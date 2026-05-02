fn clamp_category_label(text: &str) -> String {
    const MAX_CHARS: usize = 20;
    const PREFIX_CHARS: usize = 17;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut clamped: String = text.chars().take(PREFIX_CHARS).collect();
    clamped.truncate(clamped.trim_end().len());
    clamped.push_str("...");
    clamped
}

fn clamp_category_card_label(text: &str) -> String {
    const MAX_CHARS: usize = 15;
    const PREFIX_CHARS: usize = 12;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut clamped: String = text.chars().take(PREFIX_CHARS).collect();
    clamped.truncate(clamped.trim_end().len());
    clamped.push_str("...");
    clamped
}

fn clamp_metadata_source_label(text: &str) -> String {
    const MAX_CHARS: usize = 15;
    const PREFIX_CHARS: usize = 12;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut clamped: String = text.chars().take(PREFIX_CHARS).collect();
    clamped.truncate(clamped.trim_end().len());
    clamped.push_str("...");
    clamped
}

fn update_button_text(modified: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        "Update",
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(15.0),
            color: Color32::from_rgb(247, 222, 204),
            ..Default::default()
        },
    );
    if modified {
        job.append(
            "\n(Modified)",
            0.0,
            TextFormat {
                font_id: egui::FontId::proportional(9.0),
                color: Color32::from_rgb(238, 196, 168),
                ..Default::default()
            },
        );
    }
    job
}

fn paint_modified_update_badge(ui: &mut Ui, button_rect: egui::Rect) {
    let badge_size = Vec2::new(45.0, 14.0);
    let badge_rect = egui::Rect::from_min_size(
        button_rect.right_top() - egui::vec2(badge_size.x - 3.0, 3.0),
        badge_size,
    );
    ui.painter().rect(
        badge_rect,
        egui::CornerRadius::same(4),
        Color32::from_rgb(94, 57, 42),
        egui::Stroke::new(1.0, Color32::from_rgb(180, 78, 35)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        "Modified",
        egui::FontId::proportional(8.0),
        Color32::from_rgb(238, 196, 168),
    );
}

fn paint_selected_mod_count_badge(ui: &mut Ui, count: usize) {
    let text = format!("{count} selected");
    let badge_size = Vec2::new((text.len() as f32 * 5.2 + 14.0).max(66.0), 16.0);
    let content_rect = ui.max_rect();
    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(
            content_rect.right() + 16.0 - badge_size.x,
            content_rect.top() - 18.0,
        ),
        badge_size,
    );
    let painter = ui.ctx().layer_painter(ui.layer_id());
    painter.rect(
        badge_rect,
        egui::CornerRadius::same(4),
        Color32::from_rgba_premultiplied(64, 64, 64, 215),
        egui::Stroke::new(1.0, Color32::from_rgb(86, 86, 86)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(9.0),
        Color32::from_rgb(205, 210, 217),
    );
}

fn render_selected_mod_summary(ui: &mut Ui, titles: &[String], count: usize) {
    const MAX_MOD_NAME_CHARS: usize = 23;
    const CLAMPED_MOD_NAME_CHARS: usize = 20;

    if count == 0 {
        return;
    }
    paint_selected_mod_count_badge(ui, count);
    let mut rows: Vec<String> = titles.iter().take(count.min(3)).cloned().collect();
    if count > 3 {
        rows.truncate(2);
        rows.push(format!("…and {} more", count.saturating_sub(rows.len())));
    }

    for row in rows {
        let label = if row.starts_with('…') {
            format!(" {row}")
        } else {
            let display_row = if row.chars().count() > MAX_MOD_NAME_CHARS {
                let mut clamped = row
                    .chars()
                    .take(CLAMPED_MOD_NAME_CHARS)
                    .collect::<String>();
                clamped.truncate(clamped.trim_end().len());
                format!("{clamped}...")
            } else {
                row.clone()
            };
            format!("‣ {display_row}")
        };
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 17.0), Sense::hover());
        ui.painter().with_clip_rect(rect).text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            Color32::from_rgb(205, 210, 217),
        );
        response
            .on_hover_text(row)
            .on_hover_cursor(egui::CursorIcon::Default);
        ui.add_space(-10.0);
    }
    ui.add_space(6.0);
}

fn metadata_info_badge(ui: &mut Ui, text: &str) -> egui::Response {
    egui::Frame::new()
        .fill(Color32::from_rgba_premultiplied(72, 82, 94, 210))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(11.0)
                    .color(Color32::from_rgb(222, 228, 235)),
            )
        })
        .inner
}

#[derive(Clone, Copy)]
enum CategoryPickerTarget<'a> {
    Single {
        mod_id: &'a str,
        current_category_id: Option<&'a str>,
        uncategorized: bool,
    },
    Bulk {
        common_category_id: Option<&'a str>,
        all_uncategorized: bool,
    },
}

impl HestiaApp {
    fn paint_category_popup_hover(ui: &mut Ui, response: &egui::Response) {
        if response.hovered() {
            let fill = ui.visuals().widgets.hovered.bg_fill;
            ui.painter().rect_filled(
                response.rect.expand2(egui::vec2(6.0, 0.0)),
                egui::CornerRadius::same(3),
                Color32::from_rgba_premultiplied(fill.r(), fill.g(), fill.b(), 26),
            );
        }
    }

    fn category_popup_text(
        ui: &mut Ui,
        text: &str,
        count: Option<usize>,
        width: f32,
        height: f32,
        sense: Sense,
        show_hover: bool,
    ) -> egui::Response {
        let display_text = clamp_category_label(text);
        let clamped = display_text != text;
        let response = ui.allocate_response(Vec2::new(width, height), sense);
        if show_hover {
            Self::paint_category_popup_hover(ui, &response);
        }
        let text_pos = egui::pos2(response.rect.min.x, response.rect.center().y);
        let font_id = egui::FontId::new(12.0, FontFamily::Proportional);
        let galley = ui.painter().layout_no_wrap(
            display_text,
            font_id.clone(),
            ui.visuals().text_color(),
        );
        ui.painter().galley(
            egui::pos2(text_pos.x, text_pos.y - galley.size().y * 0.5),
            galley.clone(),
            ui.visuals().text_color(),
        );
        if let Some(count) = count {
            let suffix = format!(" ({count})");
            ui.painter().text(
                egui::pos2(text_pos.x + galley.size().x + 3.0, text_pos.y),
                egui::Align2::LEFT_CENTER,
                suffix,
                font_id,
                Color32::from_gray(135),
            );
        }
        if clamped {
            response.on_hover_text(text)
        } else {
            response
        }
    }

    fn category_member_count(&self, game_id: &str, category_id: &str) -> usize {
        self.state
            .mods
            .iter()
            .filter(|mod_entry| {
                mod_entry.game_id == game_id
                    && mod_entry.metadata.user.category_id.as_deref() == Some(category_id)
            })
            .count()
    }

    fn mod_category_label(&self, mod_entry: &ModEntry) -> String {
        if let Some(category_id) = mod_entry.metadata.user.category_id.as_deref() {
            if let Some(category) = self
                .state
                .categories
                .iter()
                .find(|category| category.id == category_id && category.game_id == mod_entry.game_id)
            {
                return category.name.clone();
            }
        }
        let legacy = mod_entry.metadata.user.category.trim();
        if legacy.is_empty() {
            "Uncategorized".to_string()
        } else {
            legacy.to_string()
        }
    }

    fn categories_for_game(&self, game_id: &str) -> Vec<ModCategory> {
        let mut categories: Vec<ModCategory> = self
            .state
            .categories
            .iter()
            .filter(|category| category.game_id == game_id)
            .cloned()
            .collect();
        categories.sort_by(|a, b| {
            a.order
                .cmp(&b.order)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        categories
    }

    fn compact_category_order_for_game(&mut self, game_id: &str) {
        let mut categories = self.categories_for_game(game_id);
        for (index, category) in categories.drain(..).enumerate() {
            if let Some(item) = self
                .state
                .categories
                .iter_mut()
                .find(|item| item.id == category.id)
            {
                item.order = index as i32;
            }
        }
    }

    fn restore_imported_mod_categories(&mut self, target_game_id: Option<&str>) -> bool {
        let mut changed = false;
        for index in 0..self.state.mods.len() {
            if target_game_id.is_some_and(|game_id| self.state.mods[index].game_id != game_id) {
                continue;
            }

            let category_name = self.state.mods[index].metadata.user.category.trim().to_string();
            if category_name.is_empty() {
                continue;
            }

            let game_id = self.state.mods[index].game_id.clone();
            let current_category_id = self.state.mods[index].metadata.user.category_id.clone();
            let current_category_valid = current_category_id.as_ref().is_some_and(|category_id| {
                self.state
                    .categories
                    .iter()
                    .any(|category| category.id == *category_id && category.game_id == game_id)
            });
            if current_category_valid {
                continue;
            }

            let category_id = if let Some(existing) = self
                .state
                .categories
                .iter()
                .find(|category| {
                    category.game_id == game_id
                        && category.name.eq_ignore_ascii_case(category_name.as_str())
                })
            {
                existing.id.clone()
            } else {
                let id_available = current_category_id.as_ref().is_some_and(|category_id| {
                    !self
                        .state
                        .categories
                        .iter()
                        .any(|category| category.id == *category_id)
                });
                let category_id = if id_available {
                    current_category_id.unwrap_or_default()
                } else {
                    Uuid::new_v4().to_string()
                };
                let next_order = self
                    .state
                    .categories
                    .iter()
                    .filter(|category| category.game_id == game_id)
                    .map(|category| category.order)
                    .max()
                    .map_or(0, |order| order.saturating_add(1));
                self.state.categories.push(ModCategory {
                    id: category_id.clone(),
                    game_id: game_id.clone(),
                    name: category_name.clone(),
                    order: next_order,
                });
                changed = true;
                category_id
            };

            let mod_entry = &mut self.state.mods[index];
            if mod_entry.metadata.user.category_id.as_deref() != Some(category_id.as_str())
                || mod_entry.metadata.user.category != category_name
            {
                mod_entry.metadata.user.category_id = Some(category_id);
                mod_entry.metadata.user.category = category_name;
                let _ = xxmi::save_mod_metadata(mod_entry);
                changed = true;
            }
        }
        changed
    }

    fn has_modified_update_available(mod_entry: &ModEntry) -> bool {
        if !matches!(mod_entry.update_state, ModUpdateState::ModifiedLocally) {
            return false;
        }
        let Some(source) = mod_entry.source.as_ref() else {
            return false;
        };
        if source.ignore_update_always {
            return false;
        }
        let Some(profile) = source_profile_for_compare(source) else {
            return false;
        };
        let local_sync_ts = selected_file_baseline_ts(&source.file_set)
            .or(profile.date_updated.or(Some(profile.date_modified)));
        if !matches!(
            determine_file_set_update_state(&source.file_set, local_sync_ts, &profile),
            ModUpdateState::UpdateAvailable
        ) {
            return false;
        }
        let current_signature = compute_update_signature(&source.file_set, &profile);
        !source
            .ignored_update_signature
            .as_ref()
            .is_some_and(|ignored| current_signature.as_ref() == Some(ignored))
    }

    fn mod_update_badge(mod_entry: &ModEntry) -> (&'static str, Color32) {
        if mod_has_local_changes_for_update_check(mod_entry) {
            if let Some(ignoring_label) = Self::ignored_update_short_label(mod_entry) {
                return (
                    match ignoring_label {
                        "Ignoring Once" => "Modified & Ignoring Once",
                        "Ignoring Always" => "Modified & Ignoring Always",
                        _ => "Modified Locally",
                    },
                    Color32::from_rgb(179, 133, 133),
                );
            }
        }
        if Self::has_modified_update_available(mod_entry) {
            (
                "Modified & Update Available",
                Color32::from_rgb(196, 166, 126),
            )
        } else {
            mod_update_state_badge(mod_entry.update_state)
        }
    }

    fn mod_update_badge_tooltip(mod_entry: &ModEntry) -> &'static str {
        if mod_has_local_changes_for_update_check(mod_entry) {
            if let Some(ignoring_label) = Self::ignored_update_short_label(mod_entry) {
                return match ignoring_label {
                    "Ignoring Once" => mod_update_state_tooltip(ModUpdateState::IgnoringUpdateOnce),
                    "Ignoring Always" => mod_update_state_tooltip(ModUpdateState::IgnoringUpdateAlways),
                    _ => mod_update_state_tooltip(ModUpdateState::ModifiedLocally),
                };
            }
        }
        if Self::has_modified_update_available(mod_entry) {
            mod_update_state_tooltip(ModUpdateState::UpdateAvailable)
        } else {
            mod_update_state_tooltip(mod_entry.update_state)
        }
    }

    fn ignored_update_short_label(mod_entry: &ModEntry) -> Option<&'static str> {
        let source = mod_entry.source.as_ref()?;
        if source.ignore_update_always {
            Some("Ignoring Always")
        } else if source.ignored_update_signature.is_some()
            || matches!(mod_entry.update_state, ModUpdateState::IgnoringUpdateOnce)
        {
            Some("Ignoring Once")
        } else {
            None
        }
    }

    fn modified_ignoring_detail_job(mod_entry: &ModEntry, size: f32) -> Option<LayoutJob> {
        let ignoring_label = Self::ignored_update_short_label(mod_entry)?;
        if !mod_has_local_changes_for_update_check(mod_entry) {
            return None;
        }

        let modified_color = Color32::from_rgb(179, 133, 133);
        let ignoring_color = Color32::from_rgb(181, 153, 196);
        let mut job = LayoutJob::default();
        job.append(
            "Modified",
            0.0,
            TextFormat {
                font_id: egui::FontId::proportional(size),
                color: modified_color,
                ..Default::default()
            },
        );
        job.append(
            " & ",
            0.0,
            TextFormat {
                font_id: egui::FontId::proportional(size),
                color: ignoring_color,
                ..Default::default()
            },
        );
        job.append(
            ignoring_label,
            0.0,
            TextFormat {
                font_id: egui::FontId::proportional(size),
                color: ignoring_color,
                ..Default::default()
            },
        );
        Some(job)
    }

    fn move_category_order_to_slot(&mut self, category_id: &str, slot_index: usize) -> bool {
        let Some(game_id) = self
            .state
            .categories
            .iter()
            .find(|category| category.id == category_id)
            .map(|category| category.game_id.clone())
        else {
            return false;
        };
        let mut ordered_ids: Vec<String> = self
            .categories_for_game(&game_id)
            .into_iter()
            .map(|category| category.id)
            .collect();
        let Some(current_index) = ordered_ids.iter().position(|id| id == category_id) else {
            return false;
        };
        let slot_index = slot_index.min(ordered_ids.len());
        let adjusted_index = if slot_index > current_index {
            slot_index.saturating_sub(1)
        } else {
            slot_index
        };
        if current_index == adjusted_index {
            return false;
        }
        let moving = ordered_ids.remove(current_index);
        ordered_ids.insert(adjusted_index, moving);
        for (index, id) in ordered_ids.iter().enumerate() {
            if let Some(category) = self.state.categories.iter_mut().find(|category| category.id == *id) {
                category.order = index as i32;
            }
        }
        self.compact_category_order_for_game(&game_id);
        self.save_state();
        true
    }

    fn finish_category_drag(&mut self) -> bool {
        let moved = if let (Some(dragging_id), Some(target_index)) = (
            self.dragging_category_id.clone(),
            self.dragging_category_target_index,
        ) {
            self.move_category_order_to_slot(&dragging_id, target_index)
        } else {
            false
        };
        self.dragging_category_id = None;
        self.dragging_category_target_index = None;
        moved
    }

    fn assign_mod_category(&mut self, mod_id: &str, category_id: Option<String>) {
        let category_name = category_id.as_ref().and_then(|id| {
            self.state
                .categories
                .iter()
                .find(|category| category.id == *id)
                .map(|category| category.name.clone())
        });
        let new_category = category_name.clone().unwrap_or_default();
        let Some(index) = self.state.mods.iter().position(|mod_entry| mod_entry.id == mod_id) else {
            return;
        };
        let (mod_name, old_category, changed) = {
            let mod_entry = &self.state.mods[index];
            let old_category = mod_entry.metadata.user.category.clone();
            let mod_name = mod_entry
                .metadata
                .user
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or(&mod_entry.folder_name)
                .to_string();
            let changed = mod_entry.metadata.user.category_id != category_id
                || old_category != new_category;
            (mod_name, old_category, changed)
        };
        if !changed {
            return;
        }
        {
            let mod_entry = &mut self.state.mods[index];
            mod_entry.metadata.user.category_id = category_id;
            mod_entry.metadata.user.category = new_category.clone();
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        self.log_category_change(&mod_name, &old_category, &new_category);
        self.save_state();
    }

    fn assign_selected_mods_category(&mut self, category_id: Option<String>) {
        let selected_ids: Vec<String> = self.selected_mods.iter().cloned().collect();
        if selected_ids.is_empty() {
            return;
        }
        let category_name = category_id.as_ref().and_then(|id| {
            self.state
                .categories
                .iter()
                .find(|category| category.id == *id)
                .map(|category| category.name.clone())
        });
        let new_category = category_name.unwrap_or_default();
        let mut logs = Vec::new();
        for mod_entry in self.state.mods.iter_mut().filter(|mod_entry| {
            selected_ids.iter().any(|id| id == &mod_entry.id)
        }) {
            let old_category = mod_entry.metadata.user.category.clone();
            if mod_entry.metadata.user.category_id == category_id && old_category == new_category {
                continue;
            }
            let mod_name = mod_entry
                .metadata
                .user
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or(&mod_entry.folder_name)
                .to_string();
            mod_entry.metadata.user.category_id = category_id.clone();
            mod_entry.metadata.user.category = new_category.clone();
            let _ = xxmi::save_mod_metadata(mod_entry);
            logs.push((mod_name, old_category));
        }
        if logs.is_empty() {
            return;
        }
        for (mod_name, old_category) in logs {
            self.log_category_change(&mod_name, &old_category, &new_category);
        }
        self.save_state();
    }

    fn log_category_change(&mut self, mod_name: &str, old_category: &str, new_category: &str) {
        let old_label = if old_category.trim().is_empty() {
            "(none)"
        } else {
            old_category.trim()
        };
        let new_label = if new_category.trim().is_empty() {
            "(none)"
        } else {
            new_category.trim()
        };
        self.log_action(
            "Category",
            &format!("\"{old_label}\" → \"{new_label}\" for {mod_name}"),
        );
    }

    fn create_category_for_game(&mut self, game_id: &str) -> String {
        let mut index = 1;
        let name = loop {
            let candidate = if index == 1 {
                "New Category".to_string()
            } else {
                format!("New Category {index}")
            };
            if !self
                .state
                .categories
                .iter()
                .any(|category| category.game_id == game_id && category.name.eq_ignore_ascii_case(&candidate))
            {
                break candidate;
            }
            index += 1;
        };
        let order = self
            .state
            .categories
            .iter()
            .filter(|category| category.game_id == game_id)
            .map(|category| category.order)
            .max()
            .unwrap_or(-1)
            + 1;
        let id = Uuid::new_v4().to_string();
        self.state.categories.push(ModCategory {
            id: id.clone(),
            game_id: game_id.to_string(),
            name,
            order,
        });
        self.category_rename_target_id = Some(id.clone());
        self.category_rename_name = self
            .state
            .categories
            .iter()
            .find(|category| category.id == id)
            .map(|category| category.name.clone())
            .unwrap_or_default();
        self.save_state();
        id
    }

    fn rename_category(&mut self, category_id: &str, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        let Some(category) = self
            .state
            .categories
            .iter_mut()
            .find(|category| category.id == category_id)
        else {
            return;
        };
        category.name = trimmed.to_string();
        for mod_entry in self
            .state
            .mods
            .iter_mut()
            .filter(|mod_entry| mod_entry.metadata.user.category_id.as_deref() == Some(category_id))
        {
            mod_entry.metadata.user.category = trimmed.to_string();
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        self.category_rename_target_id = None;
        self.category_rename_name.clear();
        self.save_state();
    }

    fn delete_category(&mut self, category_id: &str) {
        self.state.categories.retain(|category| category.id != category_id);
        for mod_entry in self
            .state
            .mods
            .iter_mut()
            .filter(|mod_entry| mod_entry.metadata.user.category_id.as_deref() == Some(category_id))
        {
            mod_entry.metadata.user.category_id = None;
            mod_entry.metadata.user.category.clear();
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        if self.category_rename_target_id.as_deref() == Some(category_id) {
            self.category_rename_target_id = None;
            self.category_rename_name.clear();
        }
        self.save_state();
    }

    fn render_category_picker_popup(
        &mut self,
        ui: &mut Ui,
        anchor: &egui::Response,
        popup_id: egui::Id,
        game_id: &str,
        target: CategoryPickerTarget<'_>,
    ) -> bool {
        let is_popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let was_popup_open = is_popup_open;
        let mut category_assigned = false;
        egui::Popup::menu(anchor)
            .id(popup_id)
            .width(212.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                const CATEGORY_POPUP_WIDTH: f32 = 212.0;
                const CATEGORY_ICON_WIDTH: f32 = 18.0;
                const CATEGORY_TEXT_WIDTH: f32 = 148.0;
                const CATEGORY_ROW_HEIGHT: f32 = 22.0;

                ui.set_min_width(CATEGORY_POPUP_WIDTH);
                let mut close_popup = false;
                let mut dragged_category_preview: Option<(String, egui::Rect)> = None;
                let pointer_pos = ui.ctx().pointer_latest_pos();
                let (common_category_id, all_uncategorized) = match target {
                    CategoryPickerTarget::Single {
                        current_category_id,
                        uncategorized,
                        ..
                    } => (current_category_id, uncategorized),
                    CategoryPickerTarget::Bulk {
                        common_category_id,
                        all_uncategorized,
                    } => (common_category_id, all_uncategorized),
                };

                ui.horizontal(|ui| {
                    let check_text = if all_uncategorized {
                        icon_rich(Icon::Check, 12.0, Color32::from_rgb(110, 194, 132))
                    } else {
                        RichText::new("")
                    };
                    ui.add_sized(
                        [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                        egui::Label::new(check_text).selectable(false),
                    );
                    if Self::category_popup_text(
                        ui,
                        "(none)",
                        None,
                        CATEGORY_TEXT_WIDTH,
                        CATEGORY_ROW_HEIGHT,
                        Sense::click(),
                        self.dragging_category_id.is_none(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    {
                        match target {
                            CategoryPickerTarget::Single { mod_id, .. } => {
                                self.assign_mod_category(mod_id, None);
                            }
                            CategoryPickerTarget::Bulk { .. } => {
                                self.assign_selected_mods_category(None);
                            }
                        }
                        category_assigned = true;
                        close_popup = true;
                    }
                });

                let categories = self.categories_for_game(game_id);
                let mut category_row_rects: Vec<egui::Rect> = Vec::new();
                ui.scope(|ui| {
                    ui.style_mut().spacing.scroll.floating_allocated_width = 6.0;
                    egui::ScrollArea::vertical()
                        .max_height(480.0)
                        .show(ui, |ui| {
                            for category in categories.clone() {
                                ui.horizontal(|ui| {
                                    if self.category_rename_target_id.as_deref()
                                        == Some(category.id.as_str())
                                    {
                                        ui.add_sized(
                                            [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                                            egui::Label::new(""),
                                        );
                                        let input = ui.add(
                                            TextEdit::singleline(&mut self.category_rename_name)
                                                .desired_width(CATEGORY_TEXT_WIDTH)
                                                .margin(egui::Margin::same(4)),
                                        );
                                        input.request_focus();
                                        let save_rename = input.has_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        let cancel_rename = input.has_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Escape));
                                        if save_rename {
                                            let draft = self.category_rename_name.clone();
                                            self.rename_category(&category.id, &draft);
                                        }
                                        if cancel_rename {
                                            self.category_rename_target_id = None;
                                            self.category_rename_name.clear();
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
                                        let check_text = if common_category_id
                                            == Some(category.id.as_str())
                                        {
                                            icon_rich(
                                                Icon::Check,
                                                12.0,
                                                Color32::from_rgb(110, 194, 132),
                                            )
                                        } else {
                                            RichText::new("")
                                        };
                                        ui.add_sized(
                                            [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                                            egui::Label::new(check_text).selectable(false),
                                        );
                                        let row_response = Self::category_popup_text(
                                            ui,
                                            &category.name,
                                            Some(self.category_member_count(game_id, &category.id)),
                                            CATEGORY_TEXT_WIDTH,
                                            CATEGORY_ROW_HEIGHT,
                                            Sense::click_and_drag(),
                                            self.dragging_category_id.is_none(),
                                        );
                                        if let Some(index) = categories
                                            .iter()
                                            .position(|item| item.id == category.id)
                                        {
                                            if category_row_rects.len() <= index {
                                                category_row_rects.resize(index + 1, row_response.rect);
                                            }
                                            category_row_rects[index] = row_response.rect;
                                        }
                                        let this_index = categories
                                            .iter()
                                            .position(|item| item.id == category.id);
                                        let insert_after = pointer_pos
                                            .is_some_and(|pos| pos.y > row_response.rect.center().y);
                                        let insertion_slot = this_index.map(|index| {
                                            if insert_after {
                                                index.saturating_add(1)
                                            } else {
                                                index
                                            }
                                        });
                                        if self.dragging_category_id.is_some()
                                            && self
                                                .dragging_category_id
                                                .as_ref()
                                                .is_some_and(|dragging_id| dragging_id != &category.id)
                                            && pointer_pos
                                                .is_some_and(|pos| row_response.rect.contains(pos))
                                        {
                                            if let Some(slot_index) = insertion_slot {
                                                self.dragging_category_target_index = Some(slot_index);
                                                ui.ctx().request_repaint();
                                            }
                                        }
                                        if row_response.drag_started() {
                                            self.dragging_category_id = Some(category.id.clone());
                                            self.dragging_category_target_index = this_index;
                                        }
                                        if row_response.drag_stopped()
                                            && self
                                                .dragging_category_id
                                                .as_ref()
                                                .is_some_and(|dragging_id| dragging_id == &category.id)
                                        {
                                            self.finish_category_drag();
                                        }
                                        if row_response
                                            .clone()
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                            && !row_response.dragged()
                                        {
                                            match target {
                                                CategoryPickerTarget::Single { mod_id, .. } => {
                                                    self.assign_mod_category(
                                                        mod_id,
                                                        Some(category.id.clone()),
                                                    );
                                                }
                                                CategoryPickerTarget::Bulk { .. } => {
                                                    self.assign_selected_mods_category(Some(
                                                        category.id.clone(),
                                                    ));
                                                }
                                            }
                                            category_assigned = true;
                                            close_popup = true;
                                        }
                                        if self
                                            .dragging_category_id
                                            .as_ref()
                                            .is_some_and(|dragging_id| dragging_id == &category.id)
                                        {
                                            dragged_category_preview =
                                                Some((category.name.clone(), row_response.rect));
                                        }
                                        ui.menu_button("", |ui| {
                                            if ui
                                                .button(icon_text_sized(
                                                    Icon::Pencil,
                                                    "Rename",
                                                    12.0,
                                                    12.0,
                                                ))
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.category_rename_target_id =
                                                    Some(category.id.clone());
                                                self.category_rename_name = category.name.clone();
                                            }
                                            if ui
                                                .button(icon_text_sized(
                                                    Icon::Trash2,
                                                    "Delete",
                                                    12.0,
                                                    12.0,
                                                ))
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.delete_category(&category.id);
                                                ui.close();
                                            }
                                        })
                                        .response
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    }
                                });
                            }
                            self.update_category_drag_target(
                                ui,
                                pointer_pos,
                                &category_row_rects,
                            );
                            self.paint_category_drop_indicator(ui, &category_row_rects);
                        });
                });

                ui.add_space(-2.0);
                ui.separator();
                ui.add_space(-2.0);
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                        egui::Label::new(icon_rich(Icon::Plus, 12.0, Color32::from_gray(190)))
                            .selectable(false),
                    );
                    if Self::category_popup_text(
                        ui,
                        "New Category",
                        None,
                        CATEGORY_TEXT_WIDTH,
                        CATEGORY_ROW_HEIGHT,
                        Sense::click(),
                        self.dragging_category_id.is_none(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    {
                        self.create_category_for_game(game_id);
                    }
                });

                self.paint_dragged_category_preview(ui, dragged_category_preview, popup_id);

                if close_popup {
                    ui.close();
                }
            });
        let is_popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        if was_popup_open && !is_popup_open {
            self.finish_category_drag();
            self.category_rename_target_id = None;
            self.category_rename_name.clear();
        } else if self.dragging_category_id.is_some()
            && !ui.ctx().input(|input| input.pointer.primary_down())
        {
            self.finish_category_drag();
        }
        if self.dragging_category_id.is_some()
            && ui.ctx().input(|input| input.pointer.primary_down())
        {
            ui.ctx()
                .output_mut(|output| output.cursor_icon = egui::CursorIcon::Grabbing);
        }
        category_assigned
    }

    fn update_category_drag_target(
        &mut self,
        ui: &mut Ui,
        pointer_pos: Option<egui::Pos2>,
        category_row_rects: &[egui::Rect],
    ) {
        if self.dragging_category_id.is_none()
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
                self.dragging_category_target_index = Some(0);
                ui.ctx().request_repaint();
            } else if pointer_pos.y >= bottom {
                self.dragging_category_target_index = Some(category_row_rects.len());
                ui.ctx().request_repaint();
            }
        }
    }

    fn paint_category_drop_indicator(&self, ui: &mut Ui, category_row_rects: &[egui::Rect]) {
        if self.dragging_category_id.is_none()
            || !ui.input(|input| input.pointer.primary_down())
            || category_row_rects.is_empty()
        {
            return;
        }
        let Some(target_index) = self.dragging_category_target_index else {
            return;
        };
        let clamped_index = target_index.min(category_row_rects.len());
        let line_y = if clamped_index == 0 {
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
        let dash = 4.0;
        let gap = 3.0;
        let mut x = left;
        while x < right {
            let x2 = (x + dash).min(right);
            ui.painter().line_segment(
                [egui::pos2(x, line_y), egui::pos2(x2, line_y)],
                egui::Stroke::new(
                    1.25,
                    Color32::from_rgba_premultiplied(232, 153, 118, 170),
                ),
            );
            x += dash + gap;
        }
    }

    fn paint_dragged_category_preview(
        &self,
        ui: &mut Ui,
        dragged_category_preview: Option<(String, egui::Rect)>,
        popup_id: egui::Id,
    ) {
        let Some((category_name, source_rect)) = dragged_category_preview else {
            return;
        };
        let Some(pointer_pos) = ui.ctx().pointer_latest_pos() else {
            return;
        };
        let ghost_rect = egui::Rect::from_center_size(
            pointer_pos + egui::vec2(6.0, 8.0),
            egui::vec2(source_rect.width() + 18.0, source_rect.height()),
        );
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            popup_id.with("dragging_category_ghost"),
        ));
        painter.rect(
            ghost_rect,
            egui::CornerRadius::same(6),
            Color32::from_rgba_premultiplied(44, 47, 52, 220),
            egui::Stroke::new(1.5, Color32::from_rgb(214, 104, 58)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            ghost_rect.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            clamp_category_label(&category_name),
            egui::FontId::new(12.0, FontFamily::Proportional),
            ui.visuals().text_color(),
        );
    }

    fn render_mod_category_label(&mut self, ui: &mut Ui, selected: &ModEntry) {
        let category_text = self.mod_category_label(selected);
        let response = ui.add(
            egui::Label::new(
                RichText::new(category_text)
                    .size(12.0)
                    .color(Color32::from_rgb(176, 198, 218)),
            )
            .selectable(false)
            .sense(Sense::click()),
        );
        response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);

        let popup_id = ui.id().with(("mod_category_popup", &selected.id));
        self.render_category_picker_popup(
            ui,
            &response,
            popup_id,
            &selected.game_id,
            CategoryPickerTarget::Single {
                mod_id: &selected.id,
                current_category_id: selected.metadata.user.category_id.as_deref(),
                uncategorized: selected.metadata.user.category_id.is_none()
                    && selected.metadata.user.category.trim().is_empty(),
            },
        );
    }

    fn render_mod_card_category_submenu(
        &mut self,
        ui: &mut Ui,
        mod_id: &str,
        game_id: &str,
        current_category_id: Option<&str>,
        category_label: &str,
    ) {
        let categories = self.categories_for_game(game_id);
        if categories.is_empty() {
            ui.menu_button(icon_text_sized(Icon::Tag, "Category", 12.0, 12.0), |ui| {
                ui.set_min_width(188.0);
                ui.label(
                    RichText::new(
                        "There is no category yet.\n\n1. Click a mod card to open its detail.\n2. Click \"Uncategorized\" below the mod's name.\n3. Click \"+ New Category\" and name it.",
                    )
                    .size(12.0)
                    .color(Color32::from_gray(185)),
                );
            })
            .response
            .on_hover_cursor(egui::CursorIcon::PointingHand);
            return;
        }

        ui.menu_button(icon_text_sized(Icon::Tag, "Category", 12.0, 12.0), |ui| {
            const CATEGORY_ICON_WIDTH: f32 = 18.0;
            const CATEGORY_TEXT_WIDTH: f32 = 168.0;
            const CATEGORY_ROW_HEIGHT: f32 = 22.0;
            const CATEGORY_SUBMENU_WIDTH: f32 = 204.0;
            const CATEGORY_SUBMENU_MAX_HEIGHT: f32 = 320.0;

            ui.set_min_width(CATEGORY_SUBMENU_WIDTH);
            let pointer_pos = ui.ctx().pointer_latest_pos();
            let uncategorized = current_category_id.is_none() && category_label == "Uncategorized";
            let mut category_row_rects = Vec::new();
            egui::ScrollArea::vertical()
                .max_height(CATEGORY_SUBMENU_MAX_HEIGHT)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                            egui::Label::new(if uncategorized {
                                icon_rich(Icon::Check, 12.0, Color32::from_rgb(110, 194, 132))
                            } else {
                                RichText::new("")
                            })
                            .selectable(false),
                        );
                        if Self::category_popup_text(
                            ui,
                            "(none)",
                            None,
                            CATEGORY_TEXT_WIDTH,
                            CATEGORY_ROW_HEIGHT,
                            Sense::click(),
                            self.dragging_category_id.is_none(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                        {
                            self.assign_mod_category(mod_id, None);
                            ui.close();
                        }
                    });
                    for category in categories.clone() {
                        let selected = current_category_id == Some(category.id.as_str());
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                                egui::Label::new(if selected {
                                    icon_rich(Icon::Check, 12.0, Color32::from_rgb(110, 194, 132))
                                } else {
                                    RichText::new("")
                                })
                                .selectable(false),
                            );
                            let row_response = Self::category_popup_text(
                                ui,
                                &category.name,
                                Some(self.category_member_count(game_id, &category.id)),
                                CATEGORY_TEXT_WIDTH,
                                CATEGORY_ROW_HEIGHT,
                                Sense::click_and_drag(),
                                self.dragging_category_id.is_none(),
                            );
                            if let Some(index) =
                                categories.iter().position(|item| item.id == category.id)
                            {
                                if category_row_rects.len() <= index {
                                    category_row_rects.resize(index + 1, row_response.rect);
                                }
                                category_row_rects[index] = row_response.rect;
                            }
                            let this_index =
                                categories.iter().position(|item| item.id == category.id);
                            let insert_after = pointer_pos
                                .is_some_and(|pos| pos.y > row_response.rect.center().y);
                            let insertion_slot = this_index.map(|index| {
                                if insert_after {
                                    index.saturating_add(1)
                                } else {
                                    index
                                }
                            });
                            if self.dragging_category_id.is_some()
                                && self
                                    .dragging_category_id
                                    .as_ref()
                                    .is_some_and(|dragging_id| dragging_id != &category.id)
                                && pointer_pos.is_some_and(|pos| row_response.rect.contains(pos))
                            {
                                if let Some(slot_index) = insertion_slot {
                                    self.dragging_category_target_index = Some(slot_index);
                                    ui.ctx().request_repaint();
                                }
                            }
                            if row_response.drag_started() {
                                self.dragging_category_id = Some(category.id.clone());
                                self.dragging_category_target_index = this_index;
                            }
                            if row_response.drag_stopped()
                                && self
                                    .dragging_category_id
                                    .as_ref()
                                    .is_some_and(|dragging_id| dragging_id == &category.id)
                            {
                                self.finish_category_drag();
                            }
                            if row_response
                                .clone()
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                                && !row_response.dragged()
                            {
                                self.assign_mod_category(mod_id, Some(category.id.clone()));
                                ui.close();
                            }
                            if self
                                .dragging_category_id
                                .as_ref()
                                .is_some_and(|dragging_id| dragging_id == &category.id)
                            {
                                self.paint_dragged_category_preview(
                                    ui,
                                    Some((category.name.clone(), row_response.rect)),
                                    ui.id().with(("mod_card_category_submenu", mod_id)),
                                );
                            }
                        });
                    }
                    self.update_category_drag_target(ui, pointer_pos, &category_row_rects);
                    self.paint_category_drop_indicator(ui, &category_row_rects);
                    if self.dragging_category_id.is_some()
                        && !ui.ctx().input(|input| input.pointer.primary_down())
                    {
                        self.finish_category_drag();
                    }
                });
        })
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    fn render_selected_mods_category_submenu(&mut self, ui: &mut Ui, game_id: &str) {
        let selected_category_ids: Vec<Option<String>> = self
            .state
            .mods
            .iter()
            .filter(|mod_entry| self.selected_mods.contains(&mod_entry.id))
            .map(|mod_entry| mod_entry.metadata.user.category_id.clone())
            .collect();
        let common_category_id = selected_category_ids
            .first()
            .filter(|first| {
                selected_category_ids
                    .iter()
                    .all(|category_id| category_id == *first)
            })
            .cloned()
            .flatten();
        let all_uncategorized = !selected_category_ids.is_empty()
            && selected_category_ids.iter().all(Option::is_none);
        let categories = self.categories_for_game(game_id);

        ui.menu_button(icon_text_sized(Icon::Tag, "Category", 12.0, 12.0), |ui| {
            const CATEGORY_ICON_WIDTH: f32 = 18.0;
            const CATEGORY_TEXT_WIDTH: f32 = 168.0;
            const CATEGORY_ROW_HEIGHT: f32 = 22.0;
            const CATEGORY_SUBMENU_WIDTH: f32 = 204.0;

            ui.set_min_width(CATEGORY_SUBMENU_WIDTH);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                    egui::Label::new(if all_uncategorized {
                        icon_rich(Icon::Check, 12.0, Color32::from_rgb(110, 194, 132))
                    } else {
                        RichText::new("")
                    })
                    .selectable(false),
                );
                if Self::category_popup_text(
                    ui,
                    "(none)",
                    None,
                    CATEGORY_TEXT_WIDTH,
                    CATEGORY_ROW_HEIGHT,
                    Sense::click(),
                    true,
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                {
                    self.assign_selected_mods_category(None);
                    ui.close();
                }
            });

            if categories.is_empty() {
                ui.add_space(2.0);
                ui.label(
                    RichText::new("There is no category yet.")
                        .size(12.0)
                        .color(Color32::from_gray(185)),
                );
                return;
            }

            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for category in categories {
                        let selected = common_category_id.as_deref() == Some(category.id.as_str());
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [CATEGORY_ICON_WIDTH, CATEGORY_ROW_HEIGHT],
                                egui::Label::new(if selected {
                                    icon_rich(Icon::Check, 12.0, Color32::from_rgb(110, 194, 132))
                                } else {
                                    RichText::new("")
                                })
                                .selectable(false),
                            );
                            if Self::category_popup_text(
                                ui,
                                &category.name,
                                Some(self.category_member_count(game_id, &category.id)),
                                CATEGORY_TEXT_WIDTH,
                                CATEGORY_ROW_HEIGHT,
                                Sense::click(),
                                true,
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                            {
                                self.assign_selected_mods_category(Some(category.id.clone()));
                                ui.close();
                            }
                        });
                    }
                });
        })
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    fn render_update_preference_checkboxes(&mut self, ui: &mut Ui, mod_id: &str) {
        let Some(index) = self.state.mods.iter().position(|mod_entry| mod_entry.id == mod_id)
        else {
            return;
        };
        let is_linked = self.state.mods[index]
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())
            .is_some_and(|gamebanana| gamebanana.mod_id > 0);
        if !is_linked {
            return;
        }

        let mut ignore_current_update = self.state.mods[index]
            .source
            .as_ref()
            .and_then(|source| source.ignored_update_signature.as_ref())
            .is_some();
        let mut ignore_update_always = self.state.mods[index]
            .source
            .as_ref()
            .is_some_and(|source| source.ignore_update_always);
        let mut changed = false;
        if ignore_current_update && ignore_update_always {
            ignore_current_update = false;
            if let Some(source) = self.state.mods[index].source.as_mut() {
                source.ignored_update_signature = None;
            }
            changed = true;
        }

        let ignore_once_response = ui.checkbox(&mut ignore_current_update, "Ignore update once");
        ignore_once_response.clone().on_hover_text(
            "Once a newer version is available, will automatically uncheck and process the update normally.",
        );
        ui.add_space(-6.0);
        let ignore_always_response = ui.checkbox(&mut ignore_update_always, "Ignore update always");
        ignore_always_response.clone().on_hover_text(
            "Indefinitely sets this mod's update status to \"Ignoring Update Always\" until unchecked.",
        );

        if ignore_once_response.changed() || ignore_always_response.changed() || changed {
            let mut cancel_mod = None;
            if ignore_update_always {
                if let Some(mod_entry) = self.state.mods.get_mut(index) {
                    if let Some(source) = mod_entry.source.as_mut() {
                        source.ignore_update_always = true;
                        source.ignored_update_signature = None;
                    }
                    mod_entry.update_state = ModUpdateState::IgnoringUpdateAlways;
                    cancel_mod = Some(mod_entry.clone());
                    let _ = xxmi::save_mod_metadata(mod_entry);
                }
            } else if ignore_current_update {
                let current_signature = current_update_signature_for_mod(&self.state.mods[index]);
                if let Some(mod_entry) = self.state.mods.get_mut(index) {
                    if let Some(signature) = current_signature {
                        if let Some(source) = mod_entry.source.as_mut() {
                            source.ignore_update_always = false;
                            source.ignored_update_signature = Some(signature);
                        }
                        mod_entry.update_state = ModUpdateState::IgnoringUpdateOnce;
                    } else {
                        if let Some(source) = mod_entry.source.as_mut() {
                            source.ignore_update_always = false;
                            source.ignored_update_signature = None;
                        }
                        if let Some(raw_state) = compute_raw_update_state(mod_entry) {
                            mod_entry.update_state = raw_state;
                        }
                    }
                    cancel_mod = Some(mod_entry.clone());
                    let _ = xxmi::save_mod_metadata(mod_entry);
                }
            } else if let Some(mod_entry) = self.state.mods.get_mut(index) {
                if let Some(source) = mod_entry.source.as_mut() {
                    source.ignore_update_always = false;
                    source.ignored_update_signature = None;
                }
                if let Some(raw_state) = compute_raw_update_state(mod_entry) {
                    mod_entry.update_state = raw_state;
                }
                let _ = xxmi::save_mod_metadata(mod_entry);
            }
            if let Some(mod_entry) = cancel_mod {
                self.cancel_update_process_for_mod(&mod_entry);
            }
            self.save_state();
        }
    }

    fn select_extracted_metadata_source(&mut self, mod_id: &str, source_path: &str) {
        let Some(mod_entry) = self.state.mods.iter_mut().find(|mod_entry| mod_entry.id == mod_id)
        else {
            return;
        };
        let Some(source) = mod_entry
            .metadata
            .extracted
            .text_sources
            .iter()
            .find(|source| source.path == source_path)
            .cloned()
        else {
            return;
        };

        mod_entry.metadata.user.extracted_metadata_source_path = Some(source.path.clone());
        mod_entry.metadata.extracted.description = Some(source.content);
        mod_entry.metadata.extracted.readme_path = Some(source.path);
        let _ = xxmi::save_mod_metadata(mod_entry);
        self.save_state();
    }

    fn render_workspace_view(&mut self, ui: &mut Ui) {
        if self
            .has_enabled_games()
            && self.selected_game().is_none_or(|game| !game.enabled)
        {
            if let Some((index, _)) = self
                .state
                .games
                .iter()
                .enumerate()
                .find(|(_, game)| game.enabled)
            {
                self.set_selected_game(index, ui.ctx());
            }
        }

        let available_rect = ui.available_rect_before_wrap();
        let left_width = available_rect.width() * WORKSPACE_LEFT_PANE_RATIO;
        let left_rect = egui::Rect::from_min_size(
            available_rect.min,
            egui::vec2(left_width, available_rect.height()),
        );
        let right_rect = egui::Rect::from_min_max(
            egui::pos2(left_rect.right(), available_rect.top()),
            available_rect.max,
        );

        let mut left_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let mut right_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(right_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );

        ui.spacing_mut().item_spacing.x = 0.0;
        {
            if self.has_enabled_games() {
                match self.current_view {
                    ViewMode::Library => {
                        if self.startup_scan_loading {
                            self.render_library_loading_left_pane(&mut left_ui);
                        } else {
                            self.render_mod_grid(&mut left_ui);
                        }
                    }
                    ViewMode::Browse => self.render_browse_left_pane(&mut left_ui),
                }
            } else {
                self.set_selected_mod_id(None);
                self.selected_mods.clear();
                self.mod_detail_open = false;
                self.render_blank_left_pane(&mut left_ui);
            }
            self.render_right_pane(&mut right_ui, self.current_view == ViewMode::Library);
        }
    }

    fn render_library_loading_left_pane(&mut self, ui: &mut Ui) {
        egui::Frame::new()
            .fill(Color32::from_rgba_premultiplied(36, 38, 42, 242))
            .corner_radius(egui::CornerRadius::same(0))
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.vertical_centered_justified(|ui| {
                    ui.add_space(16.0);
                    static_label(
                        ui,
                        RichText::new("Loading…")
                            .size(18.0)
                            .color(Color32::from_gray(185)),
                    );
                    ui.add_space(4.0);
                    static_label(
                        ui,
                        RichText::new("Scanning installed mods")
                            .size(12.5)
                            .color(Color32::from_gray(140)),
                    );
                });
            });
    }

    fn render_blank_left_pane(&mut self, ui: &mut Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(16.0);
                    static_label(ui, bold("No games detected or enabled").underline().size(24.0));
                    ui.add_space(-2.0);
                    static_label(
                        ui,
                        RichText::new("Ensure you have XXMI installed correctly.")
                            .color(Color32::from_gray(170))
                            .size(16.0),
                    );
                    ui.add_space(-10.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        static_label(
                            ui,
                            RichText::new("- Download XXMI: ")
                                .color(Color32::from_gray(170))
                                .size(16.0),
                        );
                        ui.hyperlink("https://github.com/SpectrumQT/XXMI-Launcher");
                    });
                    ui.add_space(8.0);
                    static_label(
                        ui,
                        RichText::new(concat!(
                            "Then go to the settings window to enable a game and fix the game path if needed.\n",
                            "- Click on the game icon to enable/disable it.\n",
                            "- Manually select a path by clicking the […] button."
                        ))
                        .color(Color32::from_gray(170))
                        .size(16.0),
                    );
                    ui.add_space(4.0);
                    if ui
                        .add_sized(
                            [156.0, 48.0],
                            egui::Button::new(bold("Open Settings").size(16.0)),
                        )
                        .clicked()
                    {
                        self.settings_open = true;
                        self.settings_tab = SettingsTab::Path;
                    }
                    ui.add_space(16.0);
                });
            });
    }

    fn render_mod_grid(&mut self, ui: &mut Ui) {
        let cards: Vec<_> = self
            .mods_for_selected_game()
            .into_iter()
            .map(|mod_entry| {
                (
                    mod_entry.id.clone(),
                    mod_entry.folder_name.clone(),
                    mod_entry.metadata.user.title.clone(),
                    mod_entry.metadata.user.cover_image.clone(),
                    mod_entry.root_path.clone(),
                    mod_entry.status.clone(),
                    mod_entry.updated_at,
                    mod_entry.unsafe_content,
                    mod_entry.update_state,
                    mod_entry
                        .source
                        .as_ref()
                        .and_then(|s| s.gamebanana.as_ref())
                        .map(|g| g.mod_id > 0 || !g.url.trim().is_empty())
                        .unwrap_or(false),
                    Self::has_modified_update_available(mod_entry),
                    mod_has_local_changes_for_update_check(mod_entry),
                    Self::ignored_update_short_label(mod_entry),
                    mod_entry.metadata.user.category_id.clone(),
                    self.mod_category_label(mod_entry),
                )
            })
            .collect();

        let selected_context_titles: Vec<String> = cards
            .iter()
            .filter(|card| self.selected_mods.contains(&card.0))
            .map(|card| {
                card.2
                    .as_deref()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or(&card.1)
                    .to_string()
            })
            .collect();

        let mut has_active = false;
        let mut has_disabled = false;
        let mut has_archived = false;
        let mut has_update_eligible = false;
        for (
            mod_id,
            _,
            _,
            _,
            _,
            status,
            _,
            _,
            update_state,
            _,
            modified_update_available,
            _,
            _,
            _,
            _,
        ) in &cards
        {
            if self.selected_mods.contains(mod_id) {
                match status {
                    ModStatus::Active => has_active = true,
                    ModStatus::Disabled => has_disabled = true,
                    ModStatus::Archived => has_archived = true,
                }
                if matches!(update_state, ModUpdateState::UpdateAvailable)
                    || (self.state.modified_update_behavior != ModifiedUpdateBehavior::HideButton
                        && *modified_update_available)
                {
                    has_update_eligible = true;
                }
            }
        }

        let mut suppress_mod_card_context_menu = false;

        egui::Frame::new()
            .fill(Color32::from_rgba_premultiplied(36, 38, 42, 242))
            .corner_radius(egui::CornerRadius::same(0))
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                let header_response = ui.horizontal(|ui| {
                    ui.set_height(41.0); // Lock height strictly to prevent expansion and jitter
                    let is_empty = self.mods_search_query.trim().is_empty();
                    let expanded = self.mods_search_expanded;
                    let how_expanded = ui.ctx().animate_bool_with_time(ui.id().with("mods_search_anim"), expanded, 0.2);
                    
                    let has_selection = !self.selected_mods.is_empty();
                    let now = ui.input(|i| i.time);
                    if has_selection {
                        // Continuously update the "last active" timestamp while selection is active
                        self.selection_empty_at = Some(now);
                    }

                    let selection_anim = ui.ctx().animate_bool_with_time(ui.id().with("batch_anim"), has_selection, 0.2);

                    ui.scope(|ui| {
                        let icon_size = 41.0;
                        let full_width = 240.0;
                        let current_width = icon_size + (full_width - icon_size) * how_expanded;

                        // Allocate the space for the whole widget
                        let (rect, _area_resp) = ui.allocate_exact_size(Vec2::new(current_width, 41.0), Sense::hover());
                        if ui.ctx().input(|i| {
                            i.pointer.secondary_clicked()
                                && i.pointer
                                    .hover_pos()
                                    .is_some_and(|pos| rect.contains(pos))
                        }) {
                            suppress_mod_card_context_menu = true;
                        }
                        
                        // 1. Draw the background bar (completely hidden at 0 expansion)
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

                        // 2. Funnel Icon Graphic & Interaction
                        let icon_pos = rect.left_center() + egui::vec2(20.5, 0.0);
                        let icon_area = egui::Rect::from_center_size(icon_pos, egui::Vec2::splat(28.0));
                        let icon_resp = ui.interact(icon_area, ui.id().with("search_toggle"), Sense::click());
                        let visibility_filtered = !self.show_enabled_mods
                            || self.state.hide_disabled
                            || self.state.hide_archived
                            || !self.show_unlinked_mods
                            || !self.show_up_to_date_mods
                            || !self.show_update_available_mods
                            || !self.show_check_skipped_mods
                            || !self.show_missing_source_mods
                            || !self.show_modified_locally_mods
                            || !self.show_ignoring_update_mods;

                        let icon_color = if expanded || !is_empty || visibility_filtered { 
                            Color32::from_rgb(214, 104, 58) // Accent color if active or filtered
                        } else if icon_resp.hovered() {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(170) 
                        };
                        
                        ui.painter().text(
                            icon_pos,
                            egui::Align2::CENTER_CENTER,
                            icon_char(Icon::Funnel),
                            egui::FontId::new(18.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                            icon_color,
                        );

                        if icon_resp.clicked() {
                            self.mods_search_expanded = !self.mods_search_expanded;
                        }
                        egui::Popup::context_menu(&icon_resp)
                            .id(ui.id().with("mods_status_filter_popup"))
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .frame(
                                egui::Frame::popup(ui.style())
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
                            .show(|ui| {
                                ui.set_min_width(170.0);
                                ui.add_sized(
                                    [ui.available_width(), 0.0],
                                    egui::Label::new(
                                        RichText::new("Toggle Visibility")
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

                                let visibility_icon_button =
                                    |ui: &mut Ui, icon: Icon, tooltip: &str| -> bool {
                                        ui.add_sized(
                                            [22.0, 22.0],
                                            egui::Button::new(icon_rich(
                                                icon,
                                                12.0,
                                                Color32::from_gray(180),
                                            )),
                                        )
                                        .on_hover_text(tooltip)
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                    };

                                ui.horizontal(|ui| {
                                    static_label(
                                        ui,
                                        bold("Mod State")
                                            .size(13.0)
                                            .underline()
                                            .color(Color32::from_gray(190)),
                                    );
                                    ui.add_space(3.0);
                                    let show_all = visibility_icon_button(
                                        ui,
                                        Icon::SquareDashedMousePointer,
                                        "Show all mod states",
                                    );
                                    ui.add_space(-12.0);
                                    let hide_all = visibility_icon_button(
                                        ui,
                                        Icon::SquareDashed,
                                        "Hide all mod states",
                                    );
                                    if show_all {
                                        self.show_enabled_mods = true;
                                        self.state.hide_disabled = false;
                                        self.state.hide_archived = false;
                                        self.selected_mods.clear();
                                        self.save_state();
                                    } else if hide_all {
                                        self.show_enabled_mods = false;
                                        self.state.hide_disabled = true;
                                        self.state.hide_archived = true;
                                        self.selected_mods.clear();
                                        self.save_state();
                                    }
                                });
                                ui.add_space(-3.0);

                                let enabled_changed = ui
                                    .checkbox(&mut self.show_enabled_mods, "Enabled mods")
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();

                                let mut show_disabled = !self.state.hide_disabled;
                                let disabled_changed = ui
                                    .checkbox(&mut show_disabled, "Disabled mods")
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                if disabled_changed {
                                    self.state.hide_disabled = !show_disabled;
                                    self.save_state();
                                }

                                let mut show_archived = !self.state.hide_archived;
                                let archived_changed = ui
                                    .checkbox(&mut show_archived, "Archived mods")
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                if archived_changed {
                                    self.state.hide_archived = !show_archived;
                                    self.save_state();
                                }

                                if enabled_changed || disabled_changed || archived_changed {
                                    self.selected_mods.clear();
                                }

                                ui.add_space(-2.0);
                                ui.separator();
                                ui.add_space(-2.0);

                                ui.horizontal(|ui| {
                                    static_label(
                                        ui,
                                        bold("Update State")
                                            .size(13.0)
                                            .underline()
                                            .color(Color32::from_gray(190)),
                                    );
                                    ui.add_space(3.0);
                                    let show_all = visibility_icon_button(
                                        ui,
                                        Icon::SquareDashedMousePointer,
                                        "Show all update states",
                                    );
                                    ui.add_space(-15.0);
                                    let hide_all = visibility_icon_button(
                                        ui,
                                        Icon::SquareDashed,
                                        "Hide all update states",
                                    );
                                    if show_all {
                                        self.show_unlinked_mods = true;
                                        self.show_up_to_date_mods = true;
                                        self.show_update_available_mods = true;
                                        self.show_check_skipped_mods = true;
                                        self.show_missing_source_mods = true;
                                        self.show_modified_locally_mods = true;
                                        self.show_ignoring_update_mods = true;
                                        self.selected_mods.clear();
                                    } else if hide_all {
                                        self.show_unlinked_mods = false;
                                        self.show_up_to_date_mods = false;
                                        self.show_update_available_mods = false;
                                        self.show_check_skipped_mods = false;
                                        self.show_missing_source_mods = false;
                                        self.show_modified_locally_mods = false;
                                        self.show_ignoring_update_mods = false;
                                        self.selected_mods.clear();
                                    }
                                });
                                ui.add_space(-3.0);

                                let unlinked_changed = ui
                                    .checkbox(&mut self.show_unlinked_mods, "Unlinked")
                                    .on_hover_text(mod_update_state_tooltip(ModUpdateState::Unlinked))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                let up_to_date_changed = ui
                                    .checkbox(&mut self.show_up_to_date_mods, "Up to Date")
                                    .on_hover_text(mod_update_state_tooltip(ModUpdateState::UpToDate))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                let update_available_changed = ui
                                    .checkbox(
                                        &mut self.show_update_available_mods,
                                        "Update Available",
                                    )
                                    .on_hover_text(mod_update_state_tooltip(ModUpdateState::UpdateAvailable))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                let check_skipped_changed = ui
                                    .checkbox(&mut self.show_check_skipped_mods, "Check Skipped")
                                    .on_hover_text(mod_update_state_tooltip(ModUpdateState::CheckSkipped))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                let missing_source_changed = ui
                                    .checkbox(
                                        &mut self.show_missing_source_mods,
                                        "Missing Source",
                                    )
                                    .on_hover_text(mod_update_state_tooltip(ModUpdateState::MissingSource))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                let modified_locally_changed = ui
                                    .checkbox(
                                        &mut self.show_modified_locally_mods,
                                        "Modified Locally",
                                    )
                                    .on_hover_text(mod_update_state_tooltip(ModUpdateState::ModifiedLocally))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();
                                let ignoring_update_changed = ui
                                    .checkbox(
                                        &mut self.show_ignoring_update_mods,
                                        "Ignoring Update",
                                    )
                                    .on_hover_text(
                                        "Shows mods that are ignoring the current update or ignoring updates until turned off.",
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .changed();

                                if unlinked_changed
                                    || up_to_date_changed
                                    || update_available_changed
                                    || check_skipped_changed
                                    || missing_source_changed
                                    || modified_locally_changed
                                    || ignoring_update_changed
                                {
                                    self.selected_mods.clear();
                                }
                            });
                        if icon_resp.hovered() {
                            icon_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                            if !expanded {
                                // Subtle background circle for the standalone icon
                                ui.painter().circle_filled(icon_pos, 14.0, Color32::from_white_alpha(15));
                            }
                        }

                        // 3. Search Text Input (reveal as bar expands)
                        if how_expanded > 0.2 {
                            let input_rect = egui::Rect::from_min_max(
                                rect.min + egui::vec2(icon_size, 0.0),
                                rect.max - egui::vec2(if !is_empty { 32.0 } else { 12.0 }, 0.0)
                            );
                            
                            let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
                            let edit_resp = child_ui.add(
                                TextEdit::singleline(&mut self.mods_search_query)
                                    .id_source(MODS_SEARCH_INPUT_ID)
                                    .hint_text(if how_expanded > 0.8 { "Filter mods..." } else { "" })
                                    .frame(false)
                                    .desired_width(input_rect.width())
                            );
                            if self.mods_search_focus_pending {
                                edit_resp.request_focus();
                                self.mods_search_focus_pending = false;
                            }
                            if edit_resp.changed() {
                                self.selected_mods.clear();
                            }
                        }

                        // 4. Clear button (fades in at the end)
                        if !is_empty && how_expanded > 0.9 {
                            let x_pos = rect.right_center() - egui::vec2(16.0, 0.0);
                            let x_area = egui::Rect::from_center_size(x_pos, egui::Vec2::splat(24.0));
                            let x_resp = ui.interact(x_area, ui.id().with("search_clear"), Sense::click());
                            let x_color = if x_resp.hovered() { Color32::from_gray(225) } else { Color32::from_gray(120) };
                            ui.painter().text(
                                x_pos,
                                egui::Align2::CENTER_CENTER,
                                icon_char(Icon::X),
                                egui::FontId::new(14.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                x_color,
                            );
                            if x_resp.clicked() {
                                self.mods_search_query.clear();
                                self.selected_mods.clear();
                            }
                            x_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                    });

                    // Floating Header Label: Disappears if expanded OR if selection is active
                    let header_visibility = (1.0 - how_expanded) * (1.0 - selection_anim);
                    if header_visibility > 0.01 {
                        ui.add_space(-4.0 * header_visibility);
                        let unit_width = 302.0 * header_visibility;
                        let (unit_rect, label_resp) = ui.allocate_exact_size(egui::vec2(unit_width, 41.0), Sense::click());
                        
                        if label_resp.clicked() {
                            self.mods_search_expanded = true;
                        }
                        label_resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                        
                        let unit_slide_left = 40.0 * (1.0 - header_visibility);
                        let content_origin = egui::pos2(
                            unit_rect.left() - unit_slide_left,
                            unit_rect.top(),
                        );
                        let text_pos = egui::pos2(content_origin.x, unit_rect.center().y);

                        let alpha = (header_visibility * 255.0) as u8;
                        ui.painter().with_clip_rect(unit_rect).text(
                            text_pos,
                            egui::Align2::LEFT_CENTER,
                            "Installed Mods",
                            egui::FontId::proportional(18.0),
                            Color32::from_rgba_premultiplied(228, 231, 235, alpha)
                        );

                        let combo_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                content_origin.x + 128.0,
                                unit_rect.top(),
                            ),
                            egui::vec2(148.0, 30.0),
                        );
                        let mut combo_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(combo_rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        combo_ui.visuals_mut().widgets.inactive.bg_fill =
                            Color32::from_rgba_premultiplied(44, 47, 52, alpha);
                        combo_ui.visuals_mut().widgets.hovered.bg_fill =
                            Color32::from_rgba_premultiplied(50, 54, 60, alpha);
                        combo_ui.visuals_mut().widgets.active.bg_fill =
                            Color32::from_rgba_premultiplied(40, 43, 48, alpha);
                        combo_ui.visuals_mut().widgets.inactive.bg_stroke.color =
                            Color32::from_rgba_premultiplied(69, 74, 81, alpha);
                        combo_ui.visuals_mut().widgets.hovered.bg_stroke.color =
                            Color32::from_rgba_premultiplied(92, 98, 107, alpha);
                        combo_ui.visuals_mut().widgets.active.bg_stroke.color =
                            Color32::from_rgba_premultiplied(92, 98, 107, alpha);
                        combo_ui.visuals_mut().widgets.inactive.corner_radius =
                            egui::CornerRadius::same(6);
                        combo_ui.visuals_mut().widgets.hovered.corner_radius =
                            egui::CornerRadius::same(6);
                        combo_ui.visuals_mut().widgets.active.corner_radius =
                            egui::CornerRadius::same(6);
                        combo_ui.visuals_mut().widgets.open.corner_radius =
                            egui::CornerRadius::same(6);
                        combo_ui.spacing_mut().icon_spacing = 4.0;

                        let mut selected_sort = self.state.library_sort;
                        let selected_text = match selected_sort {
                            LibrarySort::NameAsc => "Name A-Z",
                            LibrarySort::NameDesc => "Name Z-A",
                            LibrarySort::DateDesc => "Newest → Oldest",
                            LibrarySort::DateAsc => "Oldest → Newest",
                        };
                        let mut selected_job = LayoutJob::default();
                        selected_job.append(
                            &icon_char(Icon::ArrowDownNarrowWide).to_string(),
                            0.0,
                            TextFormat {
                                font_id: egui::FontId::new(
                                    13.0,
                                    FontFamily::Name(LUCIDE_FAMILY.into()),
                                ),
                                color: Color32::from_rgba_premultiplied(225, 229, 233, alpha),
                                ..Default::default()
                            },
                        );
                        selected_job.append(
                            "  ",
                            0.0,
                            TextFormat {
                                font_id: egui::FontId::proportional(13.0),
                                color: Color32::from_rgba_premultiplied(225, 229, 233, alpha),
                                ..Default::default()
                            },
                        );
                        selected_job.append(
                            selected_text,
                            0.0,
                            TextFormat {
                                font_id: egui::FontId::proportional(13.0),
                                color: Color32::from_rgba_premultiplied(225, 229, 233, alpha),
                                ..Default::default()
                            },
                        );
                        egui::ComboBox::from_id_salt("library_sort_combo")
                            .selected_text(selected_job)
                            .width(combo_rect.width())
                            .show_ui(&mut combo_ui, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                ui.selectable_value(&mut selected_sort, LibrarySort::NameAsc, "Name A-Z");
                                ui.selectable_value(&mut selected_sort, LibrarySort::NameDesc, "Name Z-A");
                                ui.selectable_value(&mut selected_sort, LibrarySort::DateDesc, "Newest → Oldest");
                                ui.selectable_value(&mut selected_sort, LibrarySort::DateAsc, "Oldest → Newest");
                            });
                        if selected_sort != self.state.library_sort {
                            self.state.library_sort = selected_sort;
                            self.save_state();
                        }
                    }

                    if selection_anim > 0.01 {
                        // Dynamically reduce the gap by 10px when the search bar is collapsed
                        ui.add_space(10.0 * selection_anim * how_expanded);
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 41.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0; // Total control over vertical gaps
                            ui.vertical(|ui| {
                                ui.add_space(-5.0); // Stack top margin
                                ui.spacing_mut().button_padding = egui::vec2(7.0, 5.0);
                                let radius = egui::CornerRadius::same(5);
                                ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                                ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                                ui.style_mut().visuals.widgets.active.corner_radius = radius;
                                ui.style_mut().visuals.widgets.open.corner_radius = radius;

                                let mut buttons = Vec::new();
                                if has_update_eligible { buttons.push(("update", Icon::RefreshCw, "Update")); }
                                if has_disabled { buttons.push(("enable", Icon::Check, "Enable")); }
                                if has_active { buttons.push(("disable", Icon::Ban, "Disable")); }
                                if has_active || has_disabled || has_archived { buttons.push(("category", Icon::Tag, "Category")); }
                                if has_archived { buttons.push(("restore", Icon::ArchiveRestore, "Restore")); }
                                if has_disabled { buttons.push(("archive", Icon::Archive, "Archive")); }
                                if has_active || has_disabled || has_archived { buttons.push(("delete", Icon::Trash2, "Delete")); }

                                let max_visible_buttons = if how_expanded > 0.01 {
                                    MAX_OPERATIONAL_BUTTONS_PER_ROW_WITH_SEARCHBAR
                                } else {
                                    MAX_OPERATIONAL_BUTTONS_PER_ROW
                                };
                                let (visible_buttons, overflow_buttons) = if buttons.len() > max_visible_buttons {
                                    let mut base_buttons = buttons.clone();
                                    if let Some(category_index) = base_buttons
                                        .iter()
                                        .position(|(id, _, _)| *id == "category")
                                        .filter(|index| *index >= max_visible_buttons)
                                        .filter(|_| max_visible_buttons > 0)
                                    {
                                        let category_button = base_buttons.remove(category_index);
                                        let visible_take = max_visible_buttons.saturating_sub(1);
                                        let mut visible = base_buttons
                                            .iter()
                                            .take(visible_take)
                                            .copied()
                                            .collect::<Vec<_>>();
                                        visible.push(category_button);
                                        let overflow = base_buttons
                                            .iter()
                                            .skip(visible_take)
                                            .copied()
                                            .collect::<Vec<_>>();
                                        (visible, overflow)
                                    } else {
                                        (
                                            buttons
                                                .iter()
                                                .take(max_visible_buttons)
                                                .copied()
                                                .collect::<Vec<_>>(),
                                            buttons
                                                .iter()
                                                .skip(max_visible_buttons)
                                                .copied()
                                                .collect::<Vec<_>>(),
                                        )
                                    }
                                } else {
                                    (buttons.clone(), Vec::new())
                                };

                                ui.add_space(-28.0);
                                for chunk in visible_buttons.chunks(MAX_OPERATIONAL_BUTTONS_PER_ROW) {
                                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        for &(id, icon, label) in chunk {
                                            let shortcut = match id {
                                                "enable" => Some("Ctrl+Shift+E"),
                                                "disable" => Some("Ctrl+Shift+D"),
                                                "restore" => Some("Ctrl+Shift+R"),
                                                "archive" => Some("Ctrl+Shift+A"),
                                                "delete" => Some("Delete"),
                                                _ => None,
                                            };
                                            let button_width = if id == "category" { 86.0 } else { 72.0 };
                                            let mut button = egui::Button::new(icon_text_sized(icon, label, 13.0, 13.0));
                                            if id == "update" {
                                                button = button
                                                    .fill(Color32::from_rgb(180, 78, 35))
                                                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(203, 104, 59)));
                                            }
                                            let response = ui.add_sized([button_width, 28.0], button);
                                            response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                                            if let Some(shortcut) = shortcut {
                                                response.clone().on_hover_text(format!("{label} ({shortcut})"));
                                            }
                                            if id == "category" {
                                                let popup_id = ui.id().with("batch_category_popup");
                                                let selected_ids: Vec<String> =
                                                    self.selected_mods.iter().cloned().collect();
                                                let selected_category_ids: Vec<Option<String>> = self
                                                    .state
                                                    .mods
                                                    .iter()
                                                    .filter(|mod_entry| {
                                                        selected_ids
                                                            .iter()
                                                            .any(|id| id == &mod_entry.id)
                                                    })
                                                    .map(|mod_entry| {
                                                        mod_entry.metadata.user.category_id.clone()
                                                    })
                                                    .collect();
                                                let common_category_id = selected_category_ids
                                                    .first()
                                                    .filter(|first| {
                                                        selected_category_ids
                                                            .iter()
                                                            .all(|category_id| category_id == *first)
                                                    })
                                                    .cloned()
                                                    .flatten();
                                                let all_uncategorized = !selected_category_ids.is_empty()
                                                    && selected_category_ids.iter().all(Option::is_none);
                                                let game_id = self
                                                    .selected_game()
                                                    .map(|game| game.definition.id.clone())
                                                    .unwrap_or_default();
                                                self.render_category_picker_popup(
                                                    ui,
                                                    &response,
                                                    popup_id,
                                                    &game_id,
                                                    CategoryPickerTarget::Bulk {
                                                        common_category_id: common_category_id.as_deref(),
                                                        all_uncategorized,
                                                    },
                                                );
                                                continue;
                                            }
                                            if response.clicked() {
                                                match id {
                                                    "update" => self.batch_update_selected(),
                                                    "enable" => self.batch_enable_selected(),
                                                    "disable" => self.batch_disable_selected(),
                                                    "restore" => self.batch_enable_selected(),
                                                    "archive" => self.batch_archive_selected(),
                                                    "delete" => self.batch_delete_selected(),
                                                    _ => {}
                                                }
                                            }
                                        }
                                        if !overflow_buttons.is_empty() {
                                            let overflow_response = ui.add_sized(
                                                [28.0, 28.0],
                                                egui::Button::new(icon_rich(
                                                    Icon::EllipsisVertical,
                                                    13.0,
                                                    Color32::from_gray(220),
                                                )),
                                            );
                                            overflow_response
                                                .clone()
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .on_hover_text("More");
                                            egui::Popup::menu(&overflow_response)
                                                .id(ui.id().with("batch_actions_overflow"))
                                                .width(136.0)
                                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                                .show(|ui| {
                                                    ui.spacing_mut().button_padding = egui::vec2(8.0, 5.0);
                                                    for &(id, icon, label) in &overflow_buttons {
                                                        if ui
                                                            .button(icon_text_sized(icon, label, 13.0, 13.0))
                                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                            .clicked()
                                                        {
                                                            match id {
                                                                "update" => self.batch_update_selected(),
                                                                "enable" => self.batch_enable_selected(),
                                                                "disable" => self.batch_disable_selected(),
                                                                "restore" => self.batch_enable_selected(),
                                                                "archive" => self.batch_archive_selected(),
                                                                "delete" => self.batch_delete_selected(),
                                                                _ => {}
                                                            }
                                                            ui.close();
                                                        }
                                                    }
                                                });
                                        }
                                    });
                                }
                                
                                ui.add_space(2.0);
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.add_space(6.0);
                                    let icon = icon_rich(Icon::CircleX, 11.0, Color32::from_gray(170));
                                    let response = ui.add(egui::Button::new(icon).frame(false));
                                response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                                if response.hovered() {
                                    ui.painter().circle_filled(response.rect.center(), 9.0, Color32::from_rgba_premultiplied(90, 94, 102, 60));
                                }
                                if response.clicked() {
                                    self.selected_mods.clear();
                                }
                                ui.add_space(3.0);
                                    static_label(ui, RichText::new(format!("{} selected", self.selected_mods.len())).size(12.0).color(Color32::from_gray(160)));
                                });
                            });
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.set_height(41.0); // Keep right-side layout height stable
                        // Only show stats if selection is empty AND 0.7s has passed
                        let show_stats_target = !has_selection && self.selection_empty_at.map_or(true, |t| now - t > 0.7);
                        let factor = ui.ctx().animate_bool_with_time(ui.id().with("stats_entry"), show_stats_target, if show_stats_target { 0.25 } else { 0.0 });
                        
                        if factor > 0.01 {
                            ui.add_space(20.0 * (1.0 - factor)); // Slide-left entrance
                            ui.vertical(|ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    let count_label = format!("{} mods", cards.len());
                                    let count_response = ui.add(
                                        egui::Label::new(
                                            RichText::new(count_label)
                                                .size(13.0)
                                                .color(Color32::from_gray(160).linear_multiply(factor)),
                                        )
                                        .selectable(false)
                                        .sense(Sense::click()),
                                    );
                                    count_response
                                        .clone()
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text("Select all visible mods");
                                    if count_response.clicked() {
                                        for card in &cards {
                                            self.selected_mods.insert(card.0.clone());
                                        }
                                    }
                                    
                                    let hiding_nsfw = self.state.unsafe_content_mode == UnsafeContentMode::HideShowCounter;
                                    if hiding_nsfw {
                                        if let Some(game) = self.selected_game() {
                                            let hidden_count = self.state.mods.iter().filter(|m| m.game_id == game.definition.id && m.unsafe_content).count();
                                            if hidden_count > 0 {
                                                ui.add_space(-10.0);
                                                static_label(ui, RichText::new(format!("{hidden_count} hidden for NSFW")).size(11.0).color(Color32::from_rgb(168, 112, 112).linear_multiply(factor)));
                                            }
                                        }
                                    }
                                });
                            });
                        }
                    });
                });
                if ui.ctx().input(|i| {
                    i.pointer.secondary_clicked()
                        && i.pointer
                            .hover_pos()
                            .is_some_and(|pos| header_response.response.rect.contains(pos))
                }) {
                    suppress_mod_card_context_menu = true;
                }
            });

        ui.add_space(8.0);

        let left_padding = 12.0;
        let desired_right_gap = 4.0;
        let card_spacing = 8.0;

        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.scope(|ui| {
                    // Reserve space for the floating scrollbar so it never overlaps the rightmost cards.
                    let scroll = &mut ui.style_mut().spacing.scroll;
                    if scroll.floating {
                        scroll.floating_allocated_width = scroll.bar_width + desired_right_gap;
                    } else {
                        scroll.bar_inner_margin = desired_right_gap;
                    }

                    ScrollArea::vertical().show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = card_spacing; // Gap between cards horizontally
                        ui.add_space(0.0);

                        let available = ui.available_width().max(CARD_WIDTH + left_padding);
                        ui.set_min_width(available);
                        let max_card_width = (available - left_padding).max(CARD_WIDTH);
                        let columns = ((max_card_width + card_spacing) / (CARD_WIDTH + card_spacing))
                            .floor()
                            .max(1.0) as usize;

                        let render_section_label =
                            |ui: &mut Ui, label: &str, color: Color32, count: usize| {
                                let section_height = 20.0;
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), section_height),
                                    Sense::click(),
                                );
                                let line_y = rect.center().y;
                                let line_color = Color32::from_gray(70);
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(rect.left() + left_padding, line_y),
                                        egui::pos2(rect.right() - desired_right_gap, line_y),
                                    ],
                                    egui::Stroke::new(1.0, line_color),
                                );
                                let label_text = format!("{label} ({count})");
                                let galley = ui.painter().layout_no_wrap(
                                    label_text,
                                    egui::FontId::proportional(12.0),
                                    color,
                                );
                                let text_rect =
                                    egui::Rect::from_center_size(rect.center(), galley.size());
                                ui.painter().rect_filled(
                                    text_rect.expand(6.0),
                                    6.0,
                                    Color32::from_rgba_premultiplied(28, 30, 34, 230),
                                );
                                ui.painter().galley(text_rect.min, galley, Color32::WHITE);
                                response.on_hover_cursor(egui::CursorIcon::PointingHand)
                            };

                        let library_group_mode = self.state.library_group_mode;
                        let uncategorized_first = self.state.library_uncategorized_first;
                        let selected_game_id = self
                            .selected_game()
                            .map(|game| game.definition.id.clone())
                            .unwrap_or_default();
                        let category_sections = self
                            .categories_for_game(&selected_game_id);
                        let selected_mods_snapshot = self.selected_mods.clone();

                        let mut render_cards = |ui: &mut Ui,
                                                section_cards: Vec<
                            &(
                                String,
                                String,
                                Option<String>,
                                Option<String>,
                                PathBuf,
                                ModStatus,
                                DateTime<Utc>,
                                bool,
                                ModUpdateState,
                                bool,
                                bool,
                                bool,
                                Option<&'static str>,
                                Option<String>,
                                String,
                            ),
                        >| {
                            for row in section_cards.chunks(columns) {
                                ui.horizontal_top(|ui| {
                                    ui.add_space(left_padding); // Left padding, matching header
                                    for card in row {
                                        let (
                                            mod_id,
                                            folder_name,
                                            user_title,
                                            cover_image,
                                            _root_path,
                                            status,
                                            updated_at,
                                            unsafe_content,
                                            update_state,
                                            linked,
                                            modified_update_available,
                                            modified_locally,
                                            ignoring_update_label,
                                            category_id,
                                            category_label,
                                        ) = card;
                                        let selected = self
                                            .selected_mod_id
                                            .as_deref()
                                            == Some(mod_id.as_str());
                                        let checked = self.selected_mods.contains(mod_id);
                                        let status_color = status_color(status);
                                        let card_frame = egui::Frame::new()
                                            .fill(if selected {
                                                Color32::from_rgba_premultiplied(73, 38, 31, 242)
                                            } else {
                                                Color32::from_rgba_premultiplied(33, 35, 39, 242)
                                            })
                                            .corner_radius(egui::CornerRadius::same(8))
                                            .stroke(egui::Stroke::new(
                                                1.0,
                                                if selected || checked {
                                                    Color32::from_rgb(186, 84, 43)
                                                } else {
                                                    Color32::from_rgb(60, 64, 70)
                                                },
                                            ))
                                            .inner_margin(egui::Margin::same(0))
                                            .show(ui, |ui| {
                                                ui.set_width(CARD_WIDTH);
                                                ui.vertical(|ui| {
                                                    let (rect, response) = ui.allocate_exact_size(
                                                        Vec2::new(CARD_WIDTH, 130.0),
                                                        Sense::click(),
                                                    );

                                                    if response.gained_focus() && !response.clicked() {
                                                        self.set_selected_mod_id(Some(mod_id.clone()));
                                                    }

                                                    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space)) {
                                                        self.toggle_mod_selection(mod_id, !checked);
                                                        response.request_focus();
                                                    }

                                                    ui.painter().rect_filled(
                                                        rect,
                                                        8.0,
                                                        Color32::from_rgba_premultiplied(
                                                            45, 48, 53, 242,
                                                        ),
                                                    );
                                                    let cover_texture = if let Some(cover_image) =
                                                        cover_image.as_deref()
                                                    {
                                                        if !cover_image.is_empty() {
                                                            if !self
                                                                .mod_cover_textures
                                                                .contains_key(mod_id)
                                                            {
                                                                let clip = ui.clip_rect();
                                                                let is_visible = rect.intersects(clip);
                                                                let distance = if is_visible {
                                                                    0.0
                                                                } else if rect.center().y < clip.top() {
                                                                    clip.top() - rect.center().y
                                                                } else {
                                                                    rect.center().y - clip.bottom()
                                                                };
                                                                let priority = if is_visible { 20 } else { 60 + (distance.max(0.0) as u32 / 100) };
                                                                self.queue_mod_card_thumb_load_with_priority(
                                                                    mod_id,
                                                                    priority,
                                                                );
                                                            }
                                                            self.get_mod_thumb_texture(mod_id, 2)
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        let clip = ui.clip_rect();
                                                        let is_visible = rect.intersects(clip);
                                                        let distance = if is_visible {
                                                            0.0
                                                        } else if rect.center().y < clip.top() {
                                                            clip.top() - rect.center().y
                                                        } else {
                                                            rect.center().y - clip.bottom()
                                                        };
                                                        let priority = if is_visible { 20 } else { 60 + (distance.max(0.0) as u32 / 100) };
                                                        self.queue_mod_card_thumb_load_with_priority(
                                                            mod_id,
                                                            priority,
                                                        );
                                                        self.get_mod_thumb_texture(mod_id, 2)
                                                    };
                                                    if let Some(texture) = cover_texture {
                                                        paint_thumbnail_image(
                                                            ui,
                                                            rect,
                                                            texture,
                                                            ThumbnailFit::Cover,
                                                            Color32::WHITE,
                                                            egui::CornerRadius::same(8),
                                                        );
                                                    } else if let Some(texture) =
                                                        self.mod_thumbnail_placeholder.as_ref()
                                                    {
                                                        paint_thumbnail_image(
                                                            ui,
                                                            rect,
                                                            texture,
                                                            ThumbnailFit::Contain,
                                                            Color32::from_white_alpha(51),
                                                            egui::CornerRadius::same(8),
                                                        );
                                                    } else {
                                                        ui.painter().text(
                                                            rect.center(),
                                                            egui::Align2::CENTER_CENTER,
                                                            icon_char(Icon::ImagePlus),
                                                            egui::FontId::new(
                                                                28.0,
                                                                FontFamily::Name(LUCIDE_FAMILY.into()),
                                                            ),
                                                            Color32::from_gray(150),
                                                        );
                                                    }
                                                    if *unsafe_content && self.should_censor_unsafe() {
                                                        paint_unsafe_overlay(
                                                            ui,
                                                            rect,
                                                            self.mod_thumbnail_placeholder.as_ref(),
                                                            egui::CornerRadius::same(8),
                                                        );
                                                    }
                                                    let checkbox_rect = egui::Rect::from_min_size(
                                                        rect.min + egui::vec2(6.0, 6.0),
                                                        egui::vec2(24.0, 24.0),
                                                    );
                                                    let mut checkbox_ui = ui.new_child(
                                                        egui::UiBuilder::new()
                                                            .max_rect(checkbox_rect)
                                                            .layout(
                                                                egui::Layout::left_to_right(
                                                                    egui::Align::Center,
                                                                ),
                                                            ),
                                                    );
                                                    let cb_response = larger_checkbox(&mut checkbox_ui, checked);
                                                    if cb_response.clicked() {
                                                        let modifiers = ui.input(|i| i.modifiers);
                                                        if modifiers.shift {
                                                            let mut range_selected = false;
                                                            if let Some(pivot_id) = &self.selected_mod_id {
                                                                let pivot_idx = cards.iter().position(|c| &c.0 == pivot_id);
                                                                let current_idx = cards.iter().position(|c| &c.0 == mod_id);
                                                                if let (Some(p), Some(c)) = (pivot_idx, current_idx) {
                                                                    let start = p.min(c);
                                                                    let end = p.max(c);
                                                                    for i in start..=end {
                                                                        self.selected_mods.insert(cards[i].0.clone());
                                                                    }
                                                                    range_selected = true;
                                                                }
                                                            }
                                                            if !range_selected {
                                                                self.selected_mods.insert(mod_id.clone());
                                                            }
                                                            self.set_selected_mod_id(Some(mod_id.clone()));
                                                        } else {
                                                            self.toggle_mod_selection(mod_id, !checked);
                                                        }
                                                        response.request_focus();
                                                    }
                                                    if response.clicked() {
                                                        response.request_focus();
                                                        // Space bar is used for selection toggle, so ignore it here to keep mod detail open
                                                        let is_space = ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_down(egui::Key::Space));
                                                        if !is_space {
                                                            let modifiers = ui.input(|i| i.modifiers);
                                                            if modifiers.command || modifiers.ctrl {
                                                                // Individual toggle
                                                                self.toggle_mod_selection(mod_id, !checked);
                                                            } else if modifiers.shift {
                                                                // Range selection using the active mod as anchor
                                                                let mut range_selected = false;
                                                                if let Some(pivot_id) = &self.selected_mod_id {
                                                                    let pivot_idx = cards.iter().position(|c| &c.0 == pivot_id);
                                                                    let current_idx = cards.iter().position(|c| &c.0 == mod_id);
                                                                    if let (Some(p), Some(c)) = (pivot_idx, current_idx) {
                                                                        let start = p.min(c);
                                                                        let end = p.max(c);
                                                                        for i in start..=end {
                                                                            self.selected_mods.insert(cards[i].0.clone());
                                                                        }
                                                                        range_selected = true;
                                                                    }
                                                                }
                                                                if !range_selected {
                                                                    // Fallback: if no pivot or pivot is hidden, just select this one
                                                                    self.selected_mods.insert(mod_id.clone());
                                                                }
                                                                self.set_selected_mod_id(Some(mod_id.clone()));
                                                            } else {
                                                                // Standard click: Toggle detail view
                                                                if selected {
                                                                    self.set_selected_mod_id(None);
                                                                } else {
                                                                    self.set_selected_mod_id(Some(mod_id.clone()));
                                                                }
                                                            }
                                                        }
                                                    }
                                                    ui.add_space(8.0);
                                                    egui::Frame::new()
                                                        .inner_margin(egui::Margin {
                                                            left: 8,
                                                            right: 8,
                                                            top: 0,
                                                            bottom: 0,
                                                        })
                                                        .show(ui, |ui| {
                                                            ui.vertical(|ui| {
                                                                let title = user_title
                                                                    .as_deref()
                                                                    .unwrap_or(folder_name);
                                                                let title_response = ui.add(
                                                                    egui::Label::new(
                                                                        RichText::new(title)
                                                                            .size(15.0)
                                                                            .strong()
                                                                            .color(
                                                                                Color32::from_rgb(
                                                                                    228, 231, 235,
                                                                                ),
                                                                            ),
                                                                    )
                                                                    .sense(egui::Sense::click()),
                                                                ).on_hover_cursor(egui::CursorIcon::Default);
                                                                if title_response.clicked() {
                                                                    response.request_focus();
                                                                    let modifiers = ui.input(|i| i.modifiers);
                                                                    if modifiers.command || modifiers.ctrl {
                                                                        self.toggle_mod_selection(mod_id, !checked);
                                                                    } else if modifiers.shift {
                                                                        let mut range_selected = false;
                                                                        if let Some(pivot_id) = &self.selected_mod_id {
                                                                            let pivot_idx = cards.iter().position(|c| &c.0 == pivot_id);
                                                                            let current_idx = cards.iter().position(|c| &c.0 == mod_id);
                                                                            if let (Some(p), Some(c)) = (pivot_idx, current_idx) {
                                                                                let start = p.min(c);
                                                                                let end = p.max(c);
                                                                                for i in start..=end {
                                                                                    self.selected_mods.insert(cards[i].0.clone());
                                                                                }
                                                                                range_selected = true;
                                                                            }
                                                                        }
                                                                        if !range_selected {
                                                                            self.selected_mods.insert(mod_id.clone());
                                                                        }
                                                                        self.set_selected_mod_id(Some(mod_id.clone()));
                                                                    } else {
                                                                        if selected {
                                                                            self.set_selected_mod_id(None);
                                                                        } else {
                                                                            self.set_selected_mod_id(Some(mod_id.clone()));
                                                                        }
                                                                    }
                                                                }
                                                                ui.add_space(-5.0);
                                                                ui.allocate_ui_with_layout(
                                                                    Vec2::new(
                                                                        ui.available_width(),
                                                                        0.0,
                                                                    ),
                                                                    egui::Layout::left_to_right(
                                                                        egui::Align::Center,
                                                                    ),
                                                                    |ui| {
                                                                        if *linked {
                                                                            if matches!(update_state, ModUpdateState::UpdateAvailable)
                                                                                || (self.state.modified_update_behavior != ModifiedUpdateBehavior::HideButton
                                                                                    && *modified_update_available)
                                                                            {
                                                                                ui.spacing_mut().button_padding.y = 4.0;
                                                                                let resp = ui.add(
                                                                                    egui::Button::new(
                                                                                        update_button_text(false),
                                                                                    )
                                                                                    .fill(Color32::from_rgb(180, 78, 35))
                                                                                    .corner_radius(egui::CornerRadius::same(3))
                                                                                    .min_size(Vec2::new(64.0, 4.0)),
                                                                                )
                                                                                .on_hover_text(mod_update_state_tooltip(ModUpdateState::UpdateAvailable))
                                                                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                                                                                if resp.clicked() {
                                                                                    self.queue_update_apply(mod_id);
                                                                                }
                                                                                if *modified_update_available {
                                                                                    paint_modified_update_badge(ui, resp.rect);
                                                                                }
                                                                            } else {
                                                                                if *modified_locally {
                                                                                    if let Some(ignoring_label) = ignoring_update_label {
                                                                                        ui.vertical(|ui| {
                                                                                            ui.spacing_mut().item_spacing.y = -3.0;
                                                                                            ui.add(
                                                                                                egui::Label::new(
                                                                                                    RichText::new("Modified")
                                                                                                        .size(11.0)
                                                                                                        .color(Color32::from_rgb(179, 133, 133)),
                                                                                                )
                                                                                                .selectable(false),
                                                                                            )
                                                                                            .on_hover_text(mod_update_state_tooltip(ModUpdateState::ModifiedLocally))
                                                                                            .on_hover_cursor(egui::CursorIcon::Default);
                                                                                            ui.add(
                                                                                                egui::Label::new(
                                                                                                    RichText::new(*ignoring_label)
                                                                                                        .size(11.0)
                                                                                                        .color(Color32::from_rgb(181, 153, 196)),
                                                                                                )
                                                                                                .selectable(false),
                                                                                            )
                                                                                            .on_hover_text(match *ignoring_label {
                                                                                                "Ignoring Once" => mod_update_state_tooltip(ModUpdateState::IgnoringUpdateOnce),
                                                                                                "Ignoring Always" => mod_update_state_tooltip(ModUpdateState::IgnoringUpdateAlways),
                                                                                                _ => mod_update_state_tooltip(ModUpdateState::ModifiedLocally),
                                                                                            })
                                                                                            .on_hover_cursor(egui::CursorIcon::Default);
                                                                                        });
                                                                                    } else {
                                                                                        ui.add(
                                                                                            egui::Label::new(
                                                                                                RichText::new("Modified")
                                                                                                    .size(11.0)
                                                                                                    .color(Color32::from_rgb(179, 133, 133)),
                                                                                            )
                                                                                            .selectable(false),
                                                                                        )
                                                                                        .on_hover_text(mod_update_state_tooltip(ModUpdateState::ModifiedLocally))
                                                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                                                    }
                                                                                } else {
                                                                                    let (txt, clr) = match update_state {
                                                                                        ModUpdateState::UpToDate => ("Up to Date", Color32::from_rgb(140, 174, 138)),
                                                                                        ModUpdateState::MissingSource => ("Missing", Color32::from_rgb(196, 166, 126)),
                                                                                        ModUpdateState::ModifiedLocally => ("Modified", Color32::from_rgb(179, 133, 133)),
                                                                                        ModUpdateState::CheckSkipped => ("Skipped", Color32::from_rgb(142, 153, 168)),
                                                                                        ModUpdateState::IgnoringUpdateOnce => ("Ignoring Once", Color32::from_rgb(181, 153, 196)),
                                                                                        ModUpdateState::IgnoringUpdateAlways => ("Ignoring Always", Color32::from_rgb(181, 153, 196)),
                                                                                        _ => ("", Color32::TRANSPARENT),
                                                                                    };
                                                                                    if !txt.is_empty() {
                                                                                        ui.add(
                                                                                            egui::Label::new(
                                                                                                RichText::new(txt)
                                                                                                    .size(11.0)
                                                                                                    .color(clr),
                                                                                            )
                                                                                            .selectable(false),
                                                                                        )
                                                                                        .on_hover_text(mod_update_state_tooltip(*update_state))
                                                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        ui.with_layout(
                                                                            egui::Layout::right_to_left(egui::Align::Center),
                                                                            |ui| {
                                                                                let age = mod_age_label(*updated_at);
                                                                                ui.add(
                                                                                    egui::Label::new(
                                                                                        RichText::new(age)
                                                                                            .size(12.0)
                                                                                            .color(Color32::from_gray(140)),
                                                                                    )
                                                                                    .selectable(false),
                                                                                )
                                                                                .on_hover_cursor(egui::CursorIcon::Default);
                                                                                let category_grouped = matches!(self.state.library_group_mode, LibraryGroupMode::Category);
                                                                                let show_status_on_card = category_grouped
                                                                                    && self.state.library_category_group_show_status;
                                                                                let show_category_on_card = if category_grouped {
                                                                                    !self.state.library_category_group_show_status
                                                                                } else {
                                                                                    self.state.library_status_group_show_category
                                                                                };
                                                                                if show_category_on_card {
                                                                                    let category_text = clamp_category_card_label(category_label);
                                                                                    let clamped = category_text != *category_label;
                                                                                    let category_response = ui.add(
                                                                                        egui::Label::new(
                                                                                            RichText::new(category_text)
                                                                                                .size(12.0)
                                                                                                .color(Color32::from_rgb(176, 198, 218)),
                                                                                        )
                                                                                        .selectable(false),
                                                                                    );
                                                                                    let category_response = if clamped {
                                                                                        category_response.on_hover_text(category_label)
                                                                                    } else {
                                                                                        category_response
                                                                                    };
                                                                                    category_response
                                                                                        .on_hover_cursor(egui::CursorIcon::Default);
                                                                                } else if show_status_on_card || !category_grouped {
                                                                                    ui.add(
                                                                                        egui::Label::new(
                                                                                            RichText::new(status_label(status))
                                                                                                .size(13.0)
                                                                                                .color(status_color),
                                                                                        )
                                                                                        .selectable(false),
                                                                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                                                                }
                                                                                ui.add_space(-4.0);
                                                                                ui.add(
                                                                                    egui::Label::new(
                                                                                        RichText::new("●")
                                                                                            .size(11.0)
                                                                                            .color(status_color),
                                                                                    )
                                                                                    .selectable(false),
                                                                                ).on_hover_cursor(egui::CursorIcon::Default);
                                                                            },
                                                                        );
                                                                    },
                                                                );
                                                                ui.add_space(2.0);
                                                            });
                                                        });
                                                });
                                            });
                                        let popup_id =
                                            ui.id().with(("mod_card_context_menu_popup", mod_id));
                                        let open_context_menu = ui.ctx().input(|i| {
                                            !suppress_mod_card_context_menu
                                                && i.pointer.secondary_clicked()
                                                && i.pointer
                                                    .hover_pos()
                                                    .is_some_and(|pos| {
                                                        card_frame.response.rect.contains(pos)
                                                    })
                                        });
                                        let open_batch_context_menu = open_context_menu
                                            && self.selected_mods.len() >= 2
                                            && self.selected_mods.contains(mod_id);
                                        let open_single_context_menu =
                                            open_context_menu && !open_batch_context_menu;
                                        let batch_popup_id = ui
                                            .id()
                                            .with(("selected_mods_context_menu_popup", mod_id));
                                        egui::Popup::new(
                                            batch_popup_id,
                                            ui.ctx().clone(),
                                            egui::PopupAnchor::PointerFixed,
                                            card_frame.response.layer_id,
                                        )
                                        .kind(egui::PopupKind::Menu)
                                        .layout(egui::Layout::top_down_justified(egui::Align::Min))
                                        .width(156.0)
                                        .gap(0.0)
                                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
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
                                        .open_memory(open_batch_context_menu.then_some(
                                            egui::SetOpenCommand::Bool(true),
                                        ))
                                        .show(|ui| {
                                            ui.set_min_width(156.0);
                                            let radius = egui::CornerRadius::same(3);
                                            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.active.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.open.corner_radius = radius;

                                            render_selected_mod_summary(
                                                ui,
                                                &selected_context_titles,
                                                self.selected_mods.len(),
                                            );
                                            ui.add_space(-2.0);
                                            ui.separator();
                                            ui.add_space(-2.0);

                                            if has_update_eligible
                                                && ui
                                                    .add(
                                                        egui::Button::new(icon_text_sized(
                                                            Icon::ClockPlus,
                                                            "Update",
                                                            13.0,
                                                            13.0,
                                                        ))
                                                        .fill(Color32::from_rgb(180, 78, 35))
                                                        .stroke(egui::Stroke::new(
                                                            1.0,
                                                            Color32::from_rgb(180, 78, 35),
                                                        ))
                                                        .corner_radius(radius),
                                                    )
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                            {
                                                self.batch_update_selected();
                                                ui.close();
                                            }
                                            if has_disabled && has_archived {
                                                if ui
                                                    .button(icon_text_sized(
                                                        Icon::Check,
                                                        "Enable / Restore",
                                                        12.0,
                                                        12.0,
                                                    ))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                                {
                                                    self.batch_enable_selected();
                                                    ui.close();
                                                }
                                            } else if has_disabled {
                                                if ui
                                                    .button(icon_text_sized(Icon::Check, "Enable", 12.0, 12.0))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                                {
                                                    self.batch_enable_selected();
                                                    ui.close();
                                                }
                                            } else if has_archived
                                                && ui
                                                    .button(icon_text_sized(
                                                        Icon::ArchiveRestore,
                                                        "Restore",
                                                        12.0,
                                                        12.0,
                                                    ))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                            {
                                                self.batch_enable_selected();
                                                ui.close();
                                            }
                                            if has_active
                                                && ui
                                                    .button(icon_text_sized(Icon::Ban, "Disable", 12.0, 12.0))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                            {
                                                self.batch_disable_selected();
                                                ui.close();
                                            }
                                            if has_active || has_disabled || has_archived {
                                                self.render_selected_mods_category_submenu(
                                                    ui,
                                                    &selected_game_id,
                                                );
                                            }
                                            if (has_active || has_disabled)
                                                && ui
                                                    .button(icon_text_sized(Icon::Archive, "Archive", 12.0, 12.0))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                            {
                                                self.batch_archive_selected();
                                                ui.close();
                                            }
                                            if (has_active || has_disabled || has_archived)
                                                && ui
                                                    .button(icon_text_sized(Icon::Trash2, "Delete", 12.0, 12.0))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                            {
                                                self.batch_delete_selected();
                                                ui.close();
                                            }
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
                                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
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
                                        .open_memory(open_single_context_menu.then_some(
                                            egui::SetOpenCommand::Bool(true),
                                        ))
                                        .show(|ui| {
                                            ui.set_min_width(156.0);
                                            let radius = egui::CornerRadius::same(3);
                                            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.active.corner_radius = radius;
                                            ui.style_mut().visuals.widgets.open.corner_radius = radius;
                                            let title = user_title
                                                .as_deref()
                                                .unwrap_or(folder_name);
                                            ui.add_sized(
                                                [ui.available_width(), 0.0],
                                                egui::Label::new(
                                                    RichText::new(title)
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
                                            if *linked
                                                && (matches!(update_state, ModUpdateState::UpdateAvailable)
                                                    || (self.state.modified_update_behavior != ModifiedUpdateBehavior::HideButton
                                                        && *modified_update_available))
                                            {
                                                if ui
                                                    .add(
                                                        egui::Button::new(icon_text_sized(
                                                            Icon::ClockPlus,
                                                            "Update",
                                                            13.0,
                                                            13.0,
                                                        ))
                                                        .fill(Color32::from_rgb(180, 78, 35))
                                                        .stroke(egui::Stroke::new(
                                                            1.0,
                                                            Color32::from_rgb(180, 78, 35),
                                                        ))
                                                        .corner_radius(radius),
                                                    )
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                                {
                                                    self.queue_update_apply(mod_id);
                                                    ui.close();
                                                }
                                            }
                                            match status {
                                                ModStatus::Active => {
                                                    if ui
                                                        .button(icon_text_sized(Icon::Ban, "Disable", 12.0, 12.0))
                                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                        .clicked()
                                                    {
                                                        self.disable_mod_by_id(mod_id);
                                                        ui.close();
                                                    }
                                                    self.render_mod_card_category_submenu(
                                                        ui,
                                                        mod_id,
                                                        &selected_game_id,
                                                        category_id.as_deref(),
                                                        category_label,
                                                    );
                                                    if ui
                                                        .button(icon_text_sized(Icon::Archive, "Archive", 12.0, 12.0))
                                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                        .clicked()
                                                    {
                                                        self.archive_mod_by_id(mod_id);
                                                        ui.close();
                                                    }
                                                }
                                                ModStatus::Disabled => {
                                                    if ui
                                                        .button(icon_text_sized(Icon::Check, "Enable", 12.0, 12.0))
                                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                        .clicked()
                                                    {
                                                        self.enable_or_restore_mod_by_id(mod_id);
                                                        ui.close();
                                                    }
                                                    self.render_mod_card_category_submenu(
                                                        ui,
                                                        mod_id,
                                                        &selected_game_id,
                                                        category_id.as_deref(),
                                                        category_label,
                                                    );
                                                    if ui
                                                        .button(icon_text_sized(Icon::Archive, "Archive", 12.0, 12.0))
                                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                        .clicked()
                                                    {
                                                        self.archive_mod_by_id(mod_id);
                                                        ui.close();
                                                    }
                                                }
                                                ModStatus::Archived => {
                                                    self.render_mod_card_category_submenu(
                                                        ui,
                                                        mod_id,
                                                        &selected_game_id,
                                                        category_id.as_deref(),
                                                        category_label,
                                                    );
                                                    if ui
                                                        .button(icon_text_sized(Icon::ArchiveRestore, "Restore", 12.0, 12.0))
                                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                        .clicked()
                                                    {
                                                        self.enable_or_restore_mod_by_id(mod_id);
                                                        ui.close();
                                                    }
                                                }
                                            }
                                            if ui
                                                .button(icon_text_sized(Icon::Trash2, "Delete", 12.0, 12.0))
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.delete_mod_by_id(mod_id);
                                                ui.close();
                                            }
                                            if *linked {
                                                ui.add_space(-2.0);
                                                ui.separator();
                                                ui.add_space(-6.0);
                                                self.render_update_preference_checkboxes(ui, mod_id);
                                            }
                                        });
                                    }
                                });
                                ui.add_space(4.0);
                                ui.add_space(6.0);
                            }
                        };

                        let sections = [
                            (ModStatus::Active, "Active", status_color(&ModStatus::Active)),
                            (ModStatus::Disabled, "Disabled", status_color(&ModStatus::Disabled)),
                            (ModStatus::Archived, "Archived", status_color(&ModStatus::Archived)),
                        ];

                        let mut section_select_changes: Vec<(Vec<String>, bool)> = Vec::new();
                        match library_group_mode {
                            LibraryGroupMode::None => {
                                render_cards(ui, cards.iter().collect());
                            }
                            LibraryGroupMode::Status => {
                                let visible_sections = sections
                                    .iter()
                                    .filter(|(status, _, _)| cards.iter().any(|card| card.5 == *status))
                                    .count();
                                for (status, label, color) in sections {
                                    let section_cards: Vec<_> =
                                        cards.iter().filter(|card| card.5 == status).collect();
                                    if section_cards.is_empty() {
                                        continue;
                                    }
                                    if visible_sections > 1 {
                                        let response =
                                            render_section_label(ui, label, color, section_cards.len());
                                        if response.clicked() {
                                            let ids: Vec<String> = section_cards
                                                .iter()
                                                .map(|card| card.0.clone())
                                                .collect();
                                            let all_selected = ids
                                                .iter()
                                                .all(|id| selected_mods_snapshot.contains(id));
                                            section_select_changes.push((ids, !all_selected));
                                        }
                                    }
                                    render_cards(ui, section_cards);
                                }
                            }
                            LibraryGroupMode::Category => {
                                let has_categorized = cards.iter().any(|card| {
                                    card.13.as_ref().is_some_and(|category_id| {
                                        category_sections
                                            .iter()
                                            .any(|category| category.id == *category_id)
                                    })
                                });
                                if !has_categorized {
                                    render_cards(ui, cards.iter().collect());
                                } else {
                                    let category_color = Color32::from_rgb(176, 198, 218);
                                    let mut rendered_category_ids = Vec::new();
                                    let uncategorized_cards: Vec<_> =
                                        cards.iter().filter(|card| card.13.is_none()).collect();
                                    if uncategorized_first && !uncategorized_cards.is_empty() {
                                        let response = render_section_label(
                                            ui,
                                            "Uncategorized",
                                            Color32::from_gray(165),
                                            uncategorized_cards.len(),
                                        );
                                        if response.clicked() {
                                            let ids: Vec<String> = uncategorized_cards
                                                .iter()
                                                .map(|card| card.0.clone())
                                                .collect();
                                            let all_selected = ids
                                                .iter()
                                                .all(|id| selected_mods_snapshot.contains(id));
                                            section_select_changes.push((ids, !all_selected));
                                        }
                                        render_cards(ui, uncategorized_cards.clone());
                                    }
                                    for category in category_sections {
                                        let section_cards: Vec<_> = cards
                                            .iter()
                                            .filter(|card| card.13.as_deref() == Some(category.id.as_str()))
                                            .collect();
                                        if section_cards.is_empty() {
                                            continue;
                                        }
                                        rendered_category_ids.push(category.id.clone());
                                        let response = render_section_label(
                                            ui,
                                            &category.name,
                                            category_color,
                                            section_cards.len(),
                                        );
                                        if response.clicked() {
                                            let ids: Vec<String> = section_cards
                                                .iter()
                                                .map(|card| card.0.clone())
                                                .collect();
                                            let all_selected = ids
                                                .iter()
                                                .all(|id| selected_mods_snapshot.contains(id));
                                            section_select_changes.push((ids, !all_selected));
                                        }
                                        render_cards(ui, section_cards);
                                    }
                                    if !uncategorized_first {
                                        let fallback_uncategorized_cards: Vec<_> = cards
                                            .iter()
                                            .filter(|card| {
                                                card.13.as_ref().is_none_or(|category_id| {
                                                    !rendered_category_ids
                                                        .iter()
                                                        .any(|rendered_id| rendered_id == category_id)
                                                })
                                            })
                                            .collect();
                                        if !fallback_uncategorized_cards.is_empty() {
                                            let response = render_section_label(
                                                ui,
                                                "Uncategorized",
                                                Color32::from_gray(165),
                                                fallback_uncategorized_cards.len(),
                                            );
                                            if response.clicked() {
                                                let ids: Vec<String> = fallback_uncategorized_cards
                                                    .iter()
                                                    .map(|card| card.0.clone())
                                                    .collect();
                                                let all_selected = ids
                                                    .iter()
                                                    .all(|id| selected_mods_snapshot.contains(id));
                                                section_select_changes.push((ids, !all_selected));
                                            }
                                            render_cards(ui, fallback_uncategorized_cards);
                                        }
                                    }
                                }
                            }
                        }
                        if !section_select_changes.is_empty() {
                            for (ids, should_select) in section_select_changes {
                                for id in ids {
                                    if should_select {
                                        self.selected_mods.insert(id);
                                    } else {
                                        self.selected_mods.remove(&id);
                                    }
                                }
                            }
                        }
                    });
                });
            },
        );
    }

    fn render_right_pane(&mut self, ui: &mut Ui, show_mod_detail: bool) {
        // Use the available rect and extend it to fill the pane
        let pane_rect = ui.available_rect_before_wrap();
        if ui.ctx().input(|input| input.viewport().minimized.unwrap_or(false)) {
            return;
        }
        let pane_rect_usable =
            pane_rect.width().is_finite()
                && pane_rect.height().is_finite()
                && pane_rect.width() >= 320.0
                && pane_rect.height() >= 240.0;
        if !pane_rect_usable {
            return;
        }
        self.last_right_pane_rect = Some(pane_rect);
        let mut full_rect = pane_rect;
        full_rect.max.x += COVER_RIGHT_EXTEND;
        full_rect.max.y += COVER_BOTTOM_EXTEND;

        let details_rect = pane_rect.shrink2(egui::vec2(12.0, 12.0));

        // Draw cover as background to fill entire pane
        let game_id = self
            .selected_game()
            .filter(|game| game.enabled && self.has_enabled_games())
            .map(|game| game.definition.id.clone());
        if let Some(game_id) = game_id {
            if let Some(cover_texture) = self.game_cover_textures.get(&game_id) {
                let texture_size = cover_texture.size_vec2();
                let texture_aspect = texture_size.x / texture_size.y;

                let container_rect = full_rect;
                let container_height = container_rect.height();
                let scaled_width = container_height * texture_aspect;

                if scaled_width > container_rect.width() {
                    // Image wider than container: fit height, clip sides
                    let uv_width_fraction = container_rect.width() / scaled_width;
                    let uv_x_offset = (1.0 - uv_width_fraction) / 2.0;
                    
                    ui.painter().image(
                        cover_texture.id(),
                        container_rect,
                        egui::Rect::from_min_max(
                            egui::pos2(uv_x_offset, 0.0),
                            egui::pos2(1.0 - uv_x_offset, 1.0),
                        ),
                        Color32::WHITE,
                    );
                } else {
                    // Image narrower than or equal to container: fit height, center horizontally
                    let x_offset = (container_rect.width() - scaled_width) / 2.0;
                    let centered_rect = egui::Rect::from_min_size(
                        container_rect.min + egui::vec2(x_offset, 0.0),
                        egui::vec2(scaled_width, container_height),
                    );
                    
                    ui.painter().image(
                        cover_texture.id(),
                        centered_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            }
        }

        // Dim the cover when overlay windows are open (blur-like effect).
        let overlay_open = self.settings_open
            || (show_mod_detail && self.mod_detail_open)
            || (!show_mod_detail && self.browse_detail_open)
            || self.state.show_log
            || self.state.show_tools
            || self.state.show_tasks
            || self.tool_launch_options_prompt.is_some()
            || !self.pending_imports.is_empty()
            || !self.pending_conflicts.is_empty()
            || self.browse_state.file_prompt.is_some()
            || self.browse_state.screenshot_overlay.is_some();
        if overlay_open {
            ui.painter().rect_filled(
                full_rect,
                0.0,
                Color32::from_rgba_premultiplied(14, 14, 16, 128),
            );
        }

        if !show_mod_detail {
            self.render_browse_detail_window(ui.ctx(), pane_rect);
            self.render_browse_file_prompt(ui.ctx(), details_rect);
            return;
        }

        let Some(selected) = self.selected_mod().cloned() else {
            self.render_browse_file_prompt(ui.ctx(), details_rect);
            return;
        };

        let details_offset = egui::vec2(0.0, 32.0);
        let details_pos = details_rect.min + details_offset;
        let details_size = BROWSE_DETAIL_SIZE;
        let mut mod_detail_open = self.mod_detail_open;
        let mod_detail_response = egui::Window::new("Mod Detail") // MY MOD view's mod detail GUI
            .id(egui::Id::new("mod_detail_window"))
            .default_pos(details_pos)
            .default_size(details_size)
            .open(&mut mod_detail_open)
            .title_bar(true)
            .resizable(false)
            .collapsible(true)
            .movable(true)
            .constrain_to(details_rect)
            .frame(
                egui::Frame::window(ui.style()).inner_margin(egui::Margin::same(18)),
            )
            .show(ui.ctx(), |ui| {
                let title = selected
                    .metadata
                    .user
                    .title
                    .clone()
                    .unwrap_or_else(|| selected.folder_name.clone());
                let age = mod_age_label(selected.updated_at);
                ui.horizontal_wrapped(|ui| {
                    if self.mod_detail_editing && self.mod_detail_edit_target_id.as_deref() == Some(&selected.id) {
                        let title_width = ui.fonts_mut(|f| {
                            f.layout_no_wrap(
                                title.clone(),
                                egui::TextStyle::Heading.resolve(ui.style()),
                                egui::Color32::WHITE,
                            )
                            .size()
                            .x
                        });
                        let resp = egui::Frame::NONE
                            .outer_margin(egui::Margin::symmetric(-4, -2))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.mod_detail_edit_name)
                                        .id_source(MOD_DETAIL_RENAME_INPUT_ID)
                                        .font(egui::TextStyle::Heading)
                                        .desired_width(title_width
                                            .min(ui.available_width() - 60.0) // max width of whole width left, minus 60px for the Cancel & Save buttons
                                            .max(ui.available_width() / 6.25) // min width of 16% from the whole width
                                        )
                                        .frame(false)
                                )
                            }).inner;
                        resp.request_focus();
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.mod_detail_editing = false;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.perform_mod_rename(selected.id.clone());
                        }
                        let cancel_btn = ui.add(egui::Button::new(icon_rich(Icon::X, 14.0, Color32::from_rgba_unmultiplied(160,160,160,160))).frame(false));
                        if cancel_btn.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.mod_detail_editing = false;
                        }
                        ui.add_space(-10.0);
                        let save_btn = ui.add(egui::Button::new(icon_rich(Icon::Check, 16.0, Color32::from_rgb(110, 194, 132))).frame(false));
                        if save_btn.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.perform_mod_rename(selected.id.clone());
                        }
                        // TODO: pressing enter = save
                    } else {
                        ui.heading(&title);
                        ui.add_space(-4.0);
                        let edit_btn = ui.add(egui::Button::new(icon_rich(Icon::Pencil, 9.0, Color32::from_gray(160))).frame(false));
                        edit_btn.clone().on_hover_text("Rename (F2)");
                        if edit_btn.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.start_selected_mod_rename();
                        }
                    }
                });
                let linked = selected.source.as_ref().and_then(|s| s.gamebanana.as_ref()).is_some();
                ui.add_space(-12.0);
                ui.horizontal(|ui| {
                    static_label(ui, RichText::new(status_label(&selected.status)).size(12.0).color(status_color(&selected.status)));
                    if linked {
                        ui.add_space(-4.0);
                        static_label(ui, RichText::new("/").size(12.0).color(Color32::from_gray(164)));
                        ui.add_space(-4.0);
                        if let Some(job) = Self::modified_ignoring_detail_job(&selected, 12.0) {
                            ui.add(egui::Label::new(job).selectable(false))
                                .on_hover_text(Self::mod_update_badge_tooltip(&selected))
                                .on_hover_cursor(egui::CursorIcon::Default);
                        } else {
                            let (update_text, update_color) = Self::mod_update_badge(&selected);
                            static_label(ui, RichText::new(update_text).size(12.0).color(update_color))
                                .on_hover_text(Self::mod_update_badge_tooltip(&selected));
                        }
                    }
                    ui.add_space(-4.0);
                    static_label(ui, RichText::new("/").size(12.0).color(Color32::from_gray(164)));
                    ui.add_space(-4.0);
                    self.render_mod_category_label(ui, &selected);
                });
                ui.add_space(-4.0);
                ui.horizontal(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        let modified_update_available = Self::has_modified_update_available(&selected);
                        if matches!(selected.update_state, ModUpdateState::UpdateAvailable)
                            || (self.state.modified_update_behavior != ModifiedUpdateBehavior::HideButton
                                && modified_update_available)
                        {
                            let update_response = ui.add(
                                egui::Button::new(update_button_text(false))
                                    .fill(Color32::from_rgb(180, 78, 35))
                                    .min_size(Vec2::new(78.0, 0.0))
                                    .corner_radius(egui::CornerRadius::same(6)),
                            ).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if update_response.clicked() {
                                self.queue_update_apply(&selected.id);
                            }
                            if modified_update_available {
                                paint_modified_update_badge(ui, update_response.rect);
                            }
                        }
                        let use_default_path = self.state.use_default_mods_path;
                        match selected.status {
                            ModStatus::Active => {
                            if ui
                                .add(
                                    egui::Button::new(icon_text_sized(Icon::Ban, "Disable", 12.0, 12.0))
                                        .corner_radius(egui::CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                    let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                        let name = mod_entry.folder_name.clone();
                                        (Some(xxmi::disable_mod(mod_entry)), Some(name))
                                    } else {
                                        (None, None)
                                    };
                                    if let (Some(result), Some(name)) = (result, name) {
                                        match result {
                                            Ok(()) => {
                                                self.log_action("Disabled", &name);
                                                self.set_message_ok(format!("Disabled: {name}"));
                                                self.save_state();
                                                self.refresh();
                                            }
                                            Err(err) => self.report_error(err, Some("Disable failed")),
                                        }
                                    }
                                }
                            if ui
                                .add(
                                    egui::Button::new(icon_text_sized(Icon::Archive, "Archive", 12.0, 12.0))
                                        .corner_radius(egui::CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                    if let Some(snapshot) = self.selected_mod().cloned() {
                                        self.clear_mod_image_runtime_state(&snapshot);
                                    }
                                    let game = self.selected_game().cloned();
                                    let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                        let name = mod_entry.folder_name.clone();
                                        let result = (|| -> Result<()> {
                                            let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                                            xxmi::archive_mod(mod_entry, game, use_default_path)?;
                                            Ok(())
                                        })();
                                        (Some(result), Some(name))
                                    } else {
                                        (None, None)
                                    };
                                    if let (Some(result), Some(name)) = (result, name) {
                                        match result {
                                            Ok(()) => {
                                                self.log_action("Archived", &name);
                                                self.set_message_ok(format!("Archived: {name}"));
                                                self.save_state();
                                                self.refresh();
                                            }
                                            Err(err) => self.report_error(err, Some("Archive failed")),
                                        }
                                    }
                                }
                            }
                            ModStatus::Disabled => {
                            if ui
                                .add(
                                    egui::Button::new(icon_text_sized(Icon::Check, "Enable", 12.0, 12.0))
                                        .corner_radius(egui::CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                    let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                        let name = mod_entry.folder_name.clone();
                                        (Some(xxmi::enable_mod(mod_entry)), Some(name))
                                    } else {
                                        (None, None)
                                    };
                                    if let (Some(result), Some(name)) = (result, name) {
                                        match result {
                                            Ok(()) => {
                                                self.log_action("Enabled", &name);
                                                self.set_message_ok(format!("Enabled: {name}"));
                                                self.save_state();
                                                self.refresh();
                                            }
                                            Err(err) => self.report_error(err, Some("Enable failed")),
                                        }
                                    }
                                }
                            if ui
                                .add(
                                    egui::Button::new(icon_text_sized(Icon::Archive, "Archive", 12.0, 12.0))
                                        .corner_radius(egui::CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                    if let Some(snapshot) = self.selected_mod().cloned() {
                                        self.clear_mod_image_runtime_state(&snapshot);
                                    }
                                    let game = self.selected_game().cloned();
                                    let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                        let name = mod_entry.folder_name.clone();
                                        let result = (|| -> Result<()> {
                                            let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                                            xxmi::archive_mod(mod_entry, game, use_default_path)?;
                                            Ok(())
                                        })();
                                        (Some(result), Some(name))
                                    } else {
                                        (None, None)
                                    };
                                    if let (Some(result), Some(name)) = (result, name) {
                                        match result {
                                            Ok(()) => {
                                                self.log_action("Archived", &name);
                                                self.set_message_ok(format!("Archived: {name}"));
                                                self.save_state();
                                                self.refresh();
                                            }
                                            Err(err) => self.report_error(err, Some("Archive failed")),
                                        }
                                    }
                                }
                            }
                            ModStatus::Archived => {
                            if ui
                                .add(
                                    egui::Button::new(icon_text_sized(Icon::ArchiveRestore, "Restore", 12.0, 12.0))
                                        .corner_radius(egui::CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                    let game = self.selected_game().cloned();
                                    let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                        let name = mod_entry.folder_name.clone();
                                        let result = (|| -> Result<()> {
                                            let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                                            xxmi::restore_mod(mod_entry, game, use_default_path)?;
                                            Ok(())
                                        })();
                                        (Some(result), Some(name))
                                    } else {
                                        (None, None)
                                    };
                                    if let (Some(result), Some(name)) = (result, name) {
                                        match result {
                                            Ok(()) => {
                                                self.log_action("Unarchived", &name);
                                                self.set_message_ok(format!("Unarchived: {name}"));
                                                self.save_state();
                                                self.refresh();
                                            }
                                            Err(err) => self.report_error(err, Some("Restore failed")),
                                        }
                                    }
                                }
                            }
                        }
                        if ui
                            .add(
                                egui::Button::new(icon_text_sized(Icon::Trash2, "Delete", 12.0, 12.0))
                                    .corner_radius(egui::CornerRadius::same(6)),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            let result = (|| -> Result<()> {
                                let mod_entry = self.selected_mod().cloned().ok_or_else(|| anyhow!("no mod selected"))?;
                                let action = self.delete_mod_entry(&mod_entry)?;
                                self.log_action(action, &mod_entry.folder_name);
                                self.set_message_ok(format!("{action}: {}", mod_entry.folder_name));
                                self.save_state();
                                self.refresh();
                                Ok(())
                            })();
                            if let Err(err) = result {
                                self.report_error(err, Some("Delete failed"));
                            }
                        }
                    });
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(age.clone())
                                            .size(11.5)
                                            .color(Color32::from_gray(145)),
                                    )
                                    .selectable(false),
                                ).on_hover_cursor(egui::CursorIcon::Default);
                                if let Some(author) = selected
                                    .source
                                    .as_ref()
                                    .and_then(|s| s.snapshot.as_ref())
                                    .and_then(|s| s.authors.first())
                                {
                                    ui.add_space(-6.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(author.clone())
                                                .size(11.0)
                                                .color(Color32::from_gray(168)),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    ).on_hover_cursor(egui::CursorIcon::Default);
                                }
                            });
                        },
                    );
                });
                self.mod_detail_tab = ModDetailTab::Overview;
                ScrollArea::vertical().id_salt("my_mod_detail_scroll").show(ui, |ui| {
                    if false { ui.horizontal(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let use_default_path = self.state.use_default_mods_path;
                            match selected.status {
                                ModStatus::Active => {
                                    if ui.button(icon_text_sized(Icon::Ban, "Disable", 13.0, 13.0)).clicked() {
                                        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                            let name = mod_entry.folder_name.clone();
                                            (Some(xxmi::disable_mod(mod_entry)), Some(name))
                                        } else {
                                            (None, None)
                                        };
                                        if let (Some(result), Some(name)) = (result, name) {
                                            match result {
                                                Ok(()) => {
                                                    self.log_action("Disabled", &name);
                                                    self.set_message_ok(format!("Disabled: {name}"));
                                                    self.save_state();
                                                    self.refresh();
                                                }
                                                Err(err) => self.report_error(err, Some("Disable failed")),
                                            }
                                        }
                                    }
                                    if ui.button(icon_text_sized(Icon::Archive, "Archive", 13.0, 13.0)).clicked() {
                                        if let Some(snapshot) = self.selected_mod().cloned() {
                                            self.clear_mod_image_runtime_state(&snapshot);
                                        }
                                        let game = self.selected_game().cloned();
                                        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                            let name = mod_entry.folder_name.clone();
                                            let result = (|| -> Result<()> {
                                                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                                                xxmi::archive_mod(mod_entry, game, use_default_path)?;
                                                Ok(())
                                            })();
                                            (Some(result), Some(name))
                                        } else {
                                            (None, None)
                                        };
                                        if let (Some(result), Some(name)) = (result, name) {
                                            match result {
                                                Ok(()) => {
                                                    self.log_action("Archived", &name);
                                                    self.set_message_ok(format!("Archived: {name}"));
                                                    self.save_state();
                                                    self.refresh();
                                                }
                                                Err(err) => self.report_error(err, Some("Archive failed")),
                                            }
                                        }
                                    }
                                }
                                ModStatus::Disabled => {
                                    if ui.button(icon_text_sized(Icon::Check, "Enable", 13.0, 13.0)).clicked() {
                                        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                            let name = mod_entry.folder_name.clone();
                                            (Some(xxmi::enable_mod(mod_entry)), Some(name))
                                        } else {
                                            (None, None)
                                        };
                                        if let (Some(result), Some(name)) = (result, name) {
                                            match result {
                                                Ok(()) => {
                                                    self.log_action("Enabled", &name);
                                                    self.set_message_ok(format!("Enabled: {name}"));
                                                    self.save_state();
                                                    self.refresh();
                                                }
                                                Err(err) => self.report_error(err, Some("Enable failed")),
                                            }
                                        }
                                    }
                                    if ui.button(icon_text_sized(Icon::Archive, "Archive", 13.0, 13.0)).clicked() {
                                        if let Some(snapshot) = self.selected_mod().cloned() {
                                            self.clear_mod_image_runtime_state(&snapshot);
                                        }
                                        let game = self.selected_game().cloned();
                                        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                            let name = mod_entry.folder_name.clone();
                                            let result = (|| -> Result<()> {
                                                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                                                xxmi::archive_mod(mod_entry, game, use_default_path)?;
                                                Ok(())
                                            })();
                                            (Some(result), Some(name))
                                        } else {
                                            (None, None)
                                        };
                                        if let (Some(result), Some(name)) = (result, name) {
                                            match result {
                                                Ok(()) => {
                                                    self.log_action("Archived", &name);
                                                    self.set_message_ok(format!("Archived: {name}"));
                                                    self.save_state();
                                                    self.refresh();
                                                }
                                                Err(err) => self.report_error(err, Some("Archive failed")),
                                            }
                                        }
                                    }
                                }
                                ModStatus::Archived => {
                                    if ui.button(icon_text_sized(Icon::ArchiveRestore, "Restore", 13.0, 13.0)).clicked() {
                                        let game = self.selected_game().cloned();
                                        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
                                            let name = mod_entry.folder_name.clone();
                                            let result = (|| -> Result<()> {
                                                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                                                xxmi::restore_mod(mod_entry, game, use_default_path)?;
                                                Ok(())
                                            })();
                                            (Some(result), Some(name))
                                        } else {
                                            (None, None)
                                        };
                                        if let (Some(result), Some(name)) = (result, name) {
                                            match result {
                                                Ok(()) => {
                                                    self.log_action("Unarchived", &name);
                                                    self.set_message_ok(format!("Unarchived: {name}"));
                                                    self.save_state();
                                                    self.refresh();
                                                }
                                                Err(err) => self.report_error(err, Some("Restore failed")),
                                            }
                                        }
                                    }
                                }
                            }
                            if ui.button(icon_text_sized(Icon::Trash2, "Delete", 13.0, 13.0)).clicked() {
                                let result = (|| -> Result<()> {
                                    let mod_entry = self.selected_mod().cloned().ok_or_else(|| anyhow!("no mod selected"))?;
                                    let action = self.delete_mod_entry(&mod_entry)?;
                                    self.log_action(action, &mod_entry.folder_name);
                                    self.set_message_ok(format!("{action}: {}", mod_entry.folder_name));
                                    self.save_state();
                                    self.refresh();
                                    Ok(())
                                })();
                                if let Err(err) = result {
                                    self.report_error(err, Some("Delete failed"));
                                }
                            }
                        });
                        ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    let linked = selected
                                        .source
                                        .as_ref()
                                        .and_then(|s| s.gamebanana.as_ref())
                                        .map(|g| g.mod_id > 0 || !g.url.trim().is_empty())
                                        .unwrap_or(false);
                                    if linked {
                                        if let Some(mut job) =
                                            Self::modified_ignoring_detail_job(&selected, 11.5)
                                        {
                                            job.append(
                                                &format!(" ({age})"),
                                                0.0,
                                                TextFormat {
                                                    font_id: egui::FontId::proportional(11.5),
                                                    color: Color32::from_gray(145),
                                                    ..Default::default()
                                                },
                                            );
                                            ui.add(egui::Label::new(job).selectable(false))
                                                .on_hover_text(Self::mod_update_badge_tooltip(&selected))
                                                .on_hover_cursor(egui::CursorIcon::Default);
                                        } else {
                                            let (update_text, update_color) =
                                                Self::mod_update_badge(&selected);
                                            static_label(
                                                ui,
                                                RichText::new(format!("{update_text} ({age})"))
                                                    .size(11.5)
                                                    .color(update_color),
                                            )
                                            .on_hover_text(Self::mod_update_badge_tooltip(&selected));
                                        }
                                    } else {
                                        static_label(
                                            ui,
                                            RichText::new(age.clone())
                                                .size(11.5)
                                                .color(Color32::from_gray(145)),
                                        );
                                    }
                                    if let Some(author) = selected
                                        .source
                                        .as_ref()
                                        .and_then(|s| s.snapshot.as_ref())
                                        .and_then(|s| s.authors.first())
                                    {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(author.clone())
                                                    .size(11.0)
                                                    .color(Color32::from_gray(168)),
                                            )
                                            .truncate(),
                                        ).on_hover_cursor(egui::CursorIcon::Default);
                                    }
                                });
                            },
                        );
                    }); }
                    ui.add_space(4.0);
                    let screenshot_paths = selected.metadata.user.screenshots.clone();
                    let snapshot_urls = selected
                        .source
                        .as_ref()
                        .and_then(|s| s.snapshot.as_ref())
                        .map(|s| s.preview_urls.clone())
                        .unwrap_or_default();
                    let show_source_urls = screenshot_paths.is_empty() && !snapshot_urls.is_empty();
                    if !screenshot_paths.is_empty() || show_source_urls {
                        ui.add_space(10.0);
                        ui.style_mut().spacing.scroll.floating = false;
                        let scroll_id = ui.make_persistent_id(format!("my_mod_preview_{}", selected.id));
                        let anim_id = scroll_id.with("anim");
                        let mut scroll_area = ScrollArea::horizontal()
                            .id_salt(scroll_id)
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);

                        if let Some((start_time, start_val, target_val)) =
                            ui.data(|d| d.get_temp::<(f64, f32, f32)>(anim_id))
                        {
                            let now = ui.input(|i| i.time);
                            let duration = 0.35;
                            let t = ((now - start_time) / duration).clamp(0.0, 1.0) as f32;
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
                                let mut overlay_images: Vec<MyModOverlayImage> = Vec::new();
                                if !screenshot_paths.is_empty() {
                                    for (idx, rel) in screenshot_paths.iter().enumerate() {
                                        let texture_key = format!("my-mod-shot-{}-{}", selected.id, idx);
                                        let target_height = 220.0;
                                        let width = self.mod_cover_textures.get(&texture_key)
                                            .map(|t| {
                                                let sz = t.size_vec2();
                                                if sz.y > 0.0 { target_height * (sz.x / sz.y) } else { 390.0 }
                                            })
                                            .unwrap_or(390.0);
                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, target_height), Sense::click());
                                        
                                        let clip = ui.clip_rect();
                                        let is_visible = rect.intersects(clip);
                                        let distance_x = if is_visible { 0.0 } else if rect.center().x < clip.left() { clip.left() - rect.center().x } else { rect.center().x - clip.right() };
                                        let priority = if is_visible { 10 + (idx as u32 % 10) } else { 40 + (distance_x as u32 / 10) + (idx as u32 % 10) };

                                        if !self.mod_cover_textures.contains_key(&texture_key) {
                                            let abs = selected.root_path.join(rel);
                                            self.queue_mod_image_thumb_load(
                                                texture_key.clone(),
                                                abs,
                                                priority,
                                                ThumbnailProfile::Rail,
                                            );
                                        }
                                        
                                        let texture_owned = self.get_mod_thumb_texture(&texture_key, 2).cloned()
                                            .or_else(|| {
                                                if idx == 0 {
                                                    self.get_mod_thumb_texture(&selected.id, 2).cloned()
                                                } else {
                                                    None
                                                }
                                            });

                                        if let Some(texture) = &texture_owned {
                                            paint_thumbnail_image(
                                                ui,
                                                rect,
                                                texture,
                                                ThumbnailFit::Cover,
                                                Color32::WHITE,
                                                egui::CornerRadius::same(4),
                                            );
                                            if selected.unsafe_content && self.should_censor_unsafe() {
                                                paint_unsafe_overlay(
                                                    ui,
                                                    rect,
                                                    self.mod_thumbnail_placeholder.as_ref(),
                                                    egui::CornerRadius::same(4),
                                                );
                                            }
                                        } else {
                                            ui.painter().rect_filled(rect, 4.0, Color32::from_white_alpha(12));
                                        }
                                        if response.clicked() {
                                            self.queue_overlay_full_texture(&texture_key);
                                            self.browse_state.screenshot_overlay =
                                                Some(BrowseOverlayImage { texture_key: texture_key.clone() });
                                        }

                                        // Preload hi-res for current and neighbors to match Browse view performance
                                        if rect.intersects(ui.clip_rect()) {
                                            // Only preload hi-res for visible items, and at a much lower priority than thumbnails
                                            self.queue_mod_image_full_load(texture_key.clone(), selected.root_path.join(rel), 15);
                                        }

                                        overlay_images.push(MyModOverlayImage {
                                            texture_key: texture_key.clone(),
                                            url: None,
                                            caption: None,
                                        });
                                        rects.push(rect);
                                    }
                                } else {
                                    let captions: Vec<Option<String>> = selected.source.as_ref()
                                        .and_then(|s| s.raw_profile_json.as_deref())
                                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                                        .and_then(|v| {
                                            let media = v.get("_aPreviewMedia")?;
                                            let images = media.get("_aImages")?;
                                            let arr = images.as_array()?;
                                            Some(arr.iter()
                                                .map(|img| img.get("_sCaption").and_then(|c| c.as_str()).map(|s| s.to_string()))
                                                .collect::<Vec<_>>())
                                        })
                                        .unwrap_or_default();

                                    for (idx, url) in snapshot_urls.iter().enumerate() {
                                        let key =
                                            Self::browse_thumb_texture_key(url, ThumbnailProfile::Rail);
                                        let full_key = hash64_hex(url.as_bytes());
                                        let (rect, response) = ui.allocate_exact_size(Vec2::new(390.0, 220.0), Sense::click());

                                        let clip = ui.clip_rect();
                                        let is_visible = rect.intersects(clip);
                                        let distance_x = if is_visible { 0.0 } else if rect.center().x < clip.left() { clip.left() - rect.center().x } else { rect.center().x - clip.right() };
                                        let priority = if is_visible { 10 + (idx as u32 % 10) } else { 40 + (distance_x as u32 / 10) + (idx as u32 % 10) };

                                        self.queue_browse_image_with_profile(
                                            url.clone(),
                                            None,
                                            false,
                                            ThumbnailProfile::Rail,
                                            priority,
                                        );
                                        if let Some(texture) = self.get_browse_thumb_texture(&key, 2) {
                                            paint_thumbnail_image(
                                                ui,
                                                rect,
                                                texture,
                                                ThumbnailFit::Cover,
                                                Color32::WHITE,
                                                egui::CornerRadius::same(4),
                                            );
                                        } else {
                                            ui.painter().rect_filled(rect, 4.0, Color32::from_white_alpha(12));
                                        }
                                        if response.clicked() {
                                            self.queue_overlay_full_texture(&full_key);
                                            self.browse_state.screenshot_overlay =
                                                Some(BrowseOverlayImage { texture_key: full_key.clone() });
                                        }
                                        overlay_images.push(MyModOverlayImage {
                                            texture_key: full_key,
                                            url: Some(url.clone()),
                                            caption: captions.get(idx).cloned().flatten(),
                                        });
                                        rects.push(rect);
                                    }
                                }
                                self.my_mod_overlay_images = overlay_images;
                                rects
                            });
                            ui.add_space(-44.0);
                            out
                        });

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

                            if current_offset > 1.0 {
                                let left_rect = egui::Rect::from_min_size(
                                    egui::pos2(visible_rect.min.x + 16.0, button_y),
                                    button_size,
                                );
                                let response = ui.interact(left_rect, scroll_id.with("left"), Sense::click());
                                let alpha = if response.hovered() { 240 } else { 102 };
                                ui.painter().rect_filled(left_rect, 4.0, Color32::from_black_alpha(alpha));
                                ui.painter().text(
                                    left_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    icon_char(Icon::ChevronLeft),
                                    egui::FontId::new(20.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                    Color32::WHITE,
                                );
                                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    let target = image_rects
                                        .iter()
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

                            if current_offset < max_offset - 1.0 {
                                let right_rect = egui::Rect::from_min_size(
                                    egui::pos2(visible_rect.max.x - button_size.x - 16.0, button_y),
                                    button_size,
                                );
                                let response = ui.interact(right_rect, scroll_id.with("right"), Sense::click());
                                let alpha = if response.hovered() { 240 } else { 102 };
                                ui.painter().rect_filled(right_rect, 4.0, Color32::from_black_alpha(alpha));
                                ui.painter().text(
                                    right_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    icon_char(Icon::ChevronRight),
                                    egui::FontId::new(20.0, FontFamily::Name(LUCIDE_FAMILY.into())),
                                    Color32::WHITE,
                                );
                                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    let target = image_rects
                                        .iter()
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
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        static_label(ui, bold("Description").size(14.0).underline().color(Color32::from_gray(195)));
                        if selected.metadata.extracted.requires_rabbitfx {
                            metadata_info_badge(ui, "Requires RabbitFX");
                        }
                    });
                    let markdown = mod_primary_description_markdown(&selected, &self.portable);
                    let has_description = markdown != "No description";
                    self.queue_gif_previews_for_markdown(ui.ctx(), &markdown, Some(&selected.root_path));
                    let markdown = rewrite_markdown_gif_images(&markdown, Some(&selected.root_path));
                    self.prewarm_markdown_images(&markdown);
                    self.render_markdown_with_inline_images(ui, &markdown);
                    
                    let show_metadata = match self.state.metadata_visibility {
                        MetadataVisibility::Never => false,
                        MetadataVisibility::OnlyIfNoDescription => !has_description,
                        MetadataVisibility::Always => true,
                    };

                    if show_metadata {
                        if let Some(extracted) = mod_extracted_description_markdown(&selected) {
                            if has_description {
                                ui.add_space(16.0);
                                ui.separator();
                                ui.add_space(16.0);
                            } else {
                                ui.add_space(10.0);
                            }
                            ui.horizontal(|ui| {
                                static_label(
                                    ui,
                                    bold("Metadata")
                                        .size(14.0)
                                        .underline()
                                        .color(Color32::from_gray(195)),
                                );
                                let source_path = selected
                                    .metadata
                                    .extracted
                                    .readme_path
                                    .as_deref()
                                    .filter(|path| !path.trim().is_empty());
                                let source_name = source_path.map(|source| {
                                    Path::new(source)
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or(source)
                                });
                                if let (Some(source), Some(source_name)) = (source_path, source_name) {
                                    let has_source_choices =
                                        selected.metadata.extracted.text_sources.len() > 1;
                                    let clamped_source_name =
                                        clamp_metadata_source_label(source_name);
                                    let badge_text = if has_source_choices {
                                        format!("{clamped_source_name} ▾")
                                    } else {
                                        clamped_source_name
                                    };
                                    let mut source_response =
                                        metadata_info_badge(ui, &badge_text).on_hover_text(source);
                                    if has_source_choices {
                                        source_response = source_response
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                        let popup_id = ui.id().with(("metadata_source_popup", &selected.id));
                                        egui::Popup::menu(&source_response)
                                            .id(popup_id)
                                            .width(120.0)
                                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                            .show(|ui| {
                                                ui.set_min_width(120.0);
                                                ui.spacing_mut().item_spacing.y = 3.0;
                                                egui::Frame::new()
                                                    .inner_margin(egui::Margin::same(6))
                                                    .show(ui, |ui| {
                                                        for source in selected.metadata.extracted.text_sources.clone() {
                                                            let selected_source =
                                                                selected.metadata.extracted.readme_path.as_deref()
                                                                    == Some(source.path.as_str());
                                                            let label = if source.label.trim().is_empty() {
                                                                source.path.as_str()
                                                            } else {
                                                                source.label.as_str()
                                                            };
                                                            let label = clamp_metadata_source_label(label);
                                                            let (row_rect, response) = ui.allocate_exact_size(
                                                                Vec2::new(ui.available_width(), 24.0),
                                                                Sense::click(),
                                                            );
                                                            let response = response
                                                                .on_hover_text(source.path.as_str())
                                                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                                                            let fill = if selected_source {
                                                                ui.visuals().selection.bg_fill
                                                            } else if response.hovered() {
                                                                ui.visuals().widgets.hovered.bg_fill
                                                            } else {
                                                                Color32::TRANSPARENT
                                                            };
                                                            if fill != Color32::TRANSPARENT {
                                                                ui.painter().rect_filled(
                                                                    row_rect,
                                                                    egui::CornerRadius::same(4),
                                                                    fill,
                                                                );
                                                            }
                                                            let text_color = if selected_source {
                                                                ui.visuals().selection.stroke.color
                                                            } else {
                                                                ui.visuals().text_color()
                                                            };
                                                            let text_rect = row_rect.shrink2(Vec2::new(7.0, 0.0));
                                                            ui.painter().with_clip_rect(text_rect).text(
                                                                text_rect.left_center(),
                                                                egui::Align2::LEFT_CENTER,
                                                                label.as_str(),
                                                                egui::FontId::proportional(12.0),
                                                                text_color,
                                                            );
                                                            if response.clicked() {
                                                                self.select_extracted_metadata_source(
                                                                    &selected.id,
                                                                    &source.path,
                                                                );
                                                                ui.close();
                                                            }
                                                        }
                                                    });
                                            });
                                    } else {
                                        source_response.on_hover_cursor(egui::CursorIcon::Default);
                                    }
                                }
                            });
                            // Use a simple label to preserve the literal formatting (newlines) 
                            // of the extracted text file (e.g. README.txt).
                            ui.add(egui::Label::new(
                                RichText::new(extracted)
                                    .size(13.0)
                                    .color(Color32::from_gray(175))
                            ).wrap().selectable(false)).on_hover_cursor(egui::CursorIcon::Default);
                        }
                    }
                    ui.add_space(10.0);
                    let row_height = 20.0;
                    let (row_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_height),
                        Sense::hover(),
                    );
                    let toggle_size = egui::vec2(22.0, 18.0);
                    let toggle_rect = egui::Rect::from_center_size(
                        egui::pos2(
                            row_rect.max.x - (toggle_size.x * 0.5) - 12.0,
                            row_rect.center().y,
                        ),
                        toggle_size,
                    );
                    let line_color = Color32::from_gray(98);
                    let line_y = row_rect.center().y;
                    ui.painter().line_segment(
                        [
                            egui::pos2(row_rect.min.x, line_y),
                            egui::pos2(toggle_rect.min.x - 6.0, line_y),
                        ],
                        egui::Stroke::new(1.0, line_color),
                    );
                    let toggle_icon = if self.my_mod_source_expanded {
                        Icon::ChevronsUp
                    } else {
                        Icon::ChevronsDown
                    };
                    let toggle_response = ui.put(
                        toggle_rect,
                        egui::Button::new(icon_rich(
                            toggle_icon,
                            14.0,
                            Color32::from_gray(188),
                        ))
                        .frame(false),
                    );
                    if toggle_response.hovered() {
                        let stroke = egui::Stroke {
                            width: 1.0,
                            color: line_color,
                        };
                        ui.painter().circle_stroke(
                            toggle_response.rect.center(),
                            toggle_response.rect.width() / 2.0,
                            stroke,
                        );
                    }
                    if toggle_response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.my_mod_source_expanded = !self.my_mod_source_expanded;
                    }

                    if self.my_mod_source_expanded {
                        ui.add_space(8.0);
                        let column_spacing = ui.spacing().item_spacing.x;
                        let column_width = ((ui.available_width() - column_spacing) / 2.0).max(0.0);
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(column_width, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                static_label(ui, bold("Local").size(14.0).underline().color(Color32::from_gray(195)));
                                ui.group(|ui| {
                                    let path_text = selected.root_path.display().to_string();
                                    egui::Frame::new()
                                        .fill(Color32::from_rgba_premultiplied(28, 30, 34, 230))
                                        .stroke(egui::Stroke::NONE)
                                        .corner_radius(egui::CornerRadius::same(6))
                                        .inner_margin(egui::Margin::ZERO)
                                        .show(ui, |ui| {
                                            let mut path_value = path_text.clone();
                                            let path_width = ui
                                                .painter()
                                                .layout_no_wrap(
                                                    path_text.clone(),
                                                    egui::FontId::new(12.0, FontFamily::Proportional),
                                                    Color32::from_gray(150),
                                                )
                                                .size()
                                                .x
                                                + 20.0;
                                            ScrollArea::horizontal()
                                                .id_salt(("mod_local_path_scroll", &selected.id))
                                                .max_height(24.0)
                                                .show(ui, |ui| {
                                                    ui.add(
                                                        TextEdit::singleline(&mut path_value)
                                                            .desired_width(path_width.max(ui.available_width()))
                                                            .font(egui::TextStyle::Small)
                                                            .margin(egui::Margin::ZERO)
                                                    );
                                                });
                                        });
                                    ui.add_space(6.0);
                                    ui.horizontal_centered(|ui| {
                                        if ui.button(icon_text_sized(Icon::FolderOpen, "Open in File Explorer", 12.0, 12.0)).clicked() {
                                            let _ = open_in_explorer(&selected.root_path);
                                        }
                                    });
                                });
                            },
                            );

                            ui.add_space(column_spacing);
                            ui.allocate_ui_with_layout(
                                Vec2::new(column_width, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                static_label(ui, bold("Source").size(14.0).underline().color(Color32::from_gray(195)));
                                ui.group(|ui| {
                                    let mut changed = false;
                                    let mut link_and_sync_id: Option<u64> = None;
                                    let mut unlink_requested = false;
                                    let mut open_in_browse_id: Option<u64> = None;
                                    let mut copy_gb_id: Option<u64> = None;
                                    if let Some(mod_entry) = self.selected_mod_mut() {
                                        let input_id = ui.make_persistent_id(("gb_link_input", &mod_entry.id));
                                        let mut input_str = ui.data_mut(|d| d.get_temp::<String>(input_id).unwrap_or_default());

                                        let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
                                        let gb_id = source.gamebanana.as_ref().map(|g| g.mod_id).unwrap_or(0);
                                        let is_linked = gb_id > 0;

                                        if is_linked {
                                            let gb_id_response = ui.add(
                                                egui::Label::new(
                                                    RichText::new(format!("GameBanana ID: {gb_id}"))
                                                        .size(13.0)
                                                        .strong(),
                                                )
                                                .selectable(false)
                                                .sense(Sense::click()),
                                            );
                                            if gb_id_response
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .on_hover_text("Copy GameBanana ID")
                                                .clicked()
                                            {
                                                copy_gb_id = Some(gb_id);
                                            }
                                            if let Some(ts) = source.history.updated_at {
                                                ui.add_space(-8.0);
                                                static_label(
                                                    ui,
                                                    RichText::new(format!("• Last synced: {}", mod_age_label(ts)))
                                                        .size(11.0)
                                                        .color(Color32::from_gray(145))
                                                );
                                            }
                                            ui.add_space(2.0);
                                            let resync_job = icon_text_sized(Icon::RefreshCw, "Resync", 12.0, 12.0);
                                            let unlink_job = icon_text_sized(Icon::Link2Off, "Unlink", 12.0, 12.0);
                                            let browse_job = icon_text_sized(Icon::Globe, "GameBanana Page", 12.0, 12.0);
                                            let button_padding = ui.spacing().button_padding.x * 2.0;
                                            let min_button_width = ui.spacing().interact_size.x;
                                            let inter_button_spacing = (ui.spacing().item_spacing.x - 2.0).max(0.0);
                                            let resync_width = ui.ctx().fonts_mut(|fonts| {
                                                fonts
                                                    .layout_job(resync_job.clone())
                                                    .size()
                                                    .x
                                            });
                                            let unlink_width = ui.ctx().fonts_mut(|fonts| {
                                                fonts
                                                    .layout_job(unlink_job.clone())
                                                    .size()
                                                    .x
                                            });
                                            let combined_button_width = resync_width
                                                .max(min_button_width - button_padding)
                                                + unlink_width.max(min_button_width - button_padding)
                                                + button_padding * 2.0
                                                + inter_button_spacing;
                                            ui.horizontal_centered(|ui| {
                                                if ui
                                                    .add_sized(
                                                        [combined_button_width, ui.spacing().interact_size.y],
                                                        egui::Button::new(browse_job),
                                                    )
                                                    .clicked()
                                                {
                                                    open_in_browse_id = Some(gb_id);
                                                }
                                            });
                                            ui.add_space(-3.0);
                                            ui.horizontal(|ui| {
                                                if ui.button(resync_job).clicked() {
                                                    link_and_sync_id = Some(gb_id);
                                                }
                                                ui.add_space(-2.0);
                                                if ui.button(unlink_job).clicked() {
                                                    unlink_requested = true;
                                                }
                                            });
                                            ui.add_space(2.0);
                                        } else {
                                            static_label(ui, RichText::new("Link to GameBanana to enable update tracking and metadata sync.").small().color(Color32::from_gray(160)));
                                            ui.horizontal(|ui| {
                                                let input_w = ((ui.available_width() - 84.0) / 2.0) * 1.2;
                                                ui.add(
                                                    TextEdit::singleline(&mut input_str)
                                                        .hint_text(RichText::new("URL or ID").color(Color32::from_gray(120)))
                                                        .desired_width(input_w)
                                                        .margin(egui::Margin::same(6))
                                                );
                                                ui.add_space(-6.0);
                                                let parsed_id = parse_gb_id(&input_str);
                                                if ui.add_enabled(parsed_id.is_some(), egui::Button::new(icon_text_sized(Icon::Link, "Sync Mod", 12.0, 12.0))).clicked() {
                                                    if let Some(id) = parsed_id {
                                                        link_and_sync_id = Some(id);
                                                        input_str.clear();
                                                    }
                                                }
                                            });
                                        }

                                        let show_prefs = is_linked;
                                        if show_prefs {
                                            ui.add_space(8.0);
                                            static_label(ui, RichText::new("Update Preferences:").size(12.0).color(Color32::from_gray(170)));
                                            let mut ignore_current_update = selected
                                                .source
                                                .as_ref()
                                                .and_then(|source| source.ignored_update_signature.as_ref())
                                                .is_some();
                                            let mut ignore_update_always = source.ignore_update_always;
                                            if ignore_current_update && ignore_update_always {
                                                ignore_current_update = false;
                                                source.ignored_update_signature = None;
                                                changed = true;
                                            }
                                            ui.add_space(-6.0);
                                            let ignore_once_response = ui.checkbox(&mut ignore_current_update, "Ignore update once");
                                            ignore_once_response.clone().on_hover_text(
                                                "Once a newer version is available, will automatically uncheck and process the update normally."
                                            );
                                            ui.add_space(-6.0);
                                            let ignore_always_response = ui.checkbox(&mut ignore_update_always, "Ignore update always");
                                            ignore_always_response.clone().on_hover_text(
                                                "Indefinitely sets this mod's update status to \"Ignoring Update Always\" until unchecked."
                                            );
                                            if ignore_once_response.changed() || ignore_always_response.changed() {
                                                let selected_id = selected.id.clone();
                                                if ignore_update_always {
                                                    source.ignore_update_always = true;
                                                    source.ignored_update_signature = None;
                                                    mod_entry.update_state = ModUpdateState::IgnoringUpdateAlways;
                                                    let cloned = mod_entry.clone();
                                                    let _ = xxmi::save_mod_metadata(mod_entry);
                                                    self.cancel_update_process_for_mod(&cloned);
                                                } else if ignore_current_update {
                                                    if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == selected_id) {
                                                        let current_signature = current_update_signature_for_mod(mod_entry);
                                                        if let Some(signature) = current_signature {
                                                            if let Some(source) = mod_entry.source.as_mut() {
                                                                source.ignore_update_always = false;
                                                                source.ignored_update_signature = Some(signature);
                                                            }
                                                            mod_entry.update_state = ModUpdateState::IgnoringUpdateOnce;
                                                        } else {
                                                            if let Some(source) = mod_entry.source.as_mut() {
                                                                source.ignore_update_always = false;
                                                                source.ignored_update_signature = None;
                                                            }
                                                            if let Some(raw_state) = compute_raw_update_state(mod_entry) {
                                                                mod_entry.update_state = raw_state;
                                                            }
                                                        }
                                                        let cloned = mod_entry.clone();
                                                        let _ = xxmi::save_mod_metadata(mod_entry);
                                                        self.cancel_update_process_for_mod(&cloned);
                                                    }
                                                } else if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == selected_id) {
                                                    if let Some(source) = mod_entry.source.as_mut() {
                                                        source.ignore_update_always = false;
                                                        source.ignored_update_signature = None;
                                                    }
                                                    if let Some(raw_state) = compute_raw_update_state(mod_entry) {
                                                        mod_entry.update_state = raw_state;
                                                    }
                                                    let _ = xxmi::save_mod_metadata(mod_entry);
                                                }
                                                self.save_state();
                                            }
                                        }

                                        ui.data_mut(|d| d.insert_temp(input_id, input_str));
                                    }

                                    if let Some(id) = open_in_browse_id {
                                        self.open_linked_mod_in_browse(id);
                                    }
                                    if let Some(id) = copy_gb_id {
                                        ui.ctx().copy_text(id.to_string());
                                        self.set_message_ok("GameBanana ID copied");
                                    }
                                    if unlink_requested {
                                        if let Some(mod_entry) = self.selected_mod_mut() {
                                            if let Some(source) = mod_entry.source.as_mut() {
                                                source.gamebanana = None;
                                                mod_entry.update_state = ModUpdateState::Unlinked;
                                                let _ = xxmi::save_mod_metadata(mod_entry);
                                            }
                                        }
                                        self.save_state();
                                    }

                                    if let Some(id) = link_and_sync_id {
                                        let mut mod_entry_id = None;
                                        if let Some(mod_entry) = self.selected_mod_mut() {
                                            let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
                                            source.gamebanana = Some(GameBananaLink {
                                                mod_id: id,
                                                url: gamebanana::browser_url(id),
                                            });
                                            source.history.updated_at = Some(Utc::now());

                                            mod_entry_id = Some(mod_entry.id.clone());
                                            let _ = xxmi::save_mod_metadata(mod_entry);
                                        }

                                        if let Some(m_id) = mod_entry_id {
                                            self.queue_update_check_for_mod(&m_id);
                                            self.set_message_ok("Syncing with GameBanana…");
                                        }
                                        self.save_state();
                                    }

                                    if changed {
                                        if let Some(mod_entry) = self.selected_mod_mut() {
                                            let _ = xxmi::save_mod_metadata(mod_entry);
                                        }
                                        self.save_state();
                                    }
                                });
                            },
                            );
                        });
                    }
                });

            });

        if self.mod_detail_focus_requested {
            if let Some(inner) = mod_detail_response {
                ui.ctx().move_to_top(inner.response.layer_id);
                self.mod_detail_focus_requested = false;
            }
        }

        self.mod_detail_open = mod_detail_open;
        if !self.mod_detail_open {
            self.set_selected_mod_id(None);
        }
        self.render_browse_screenshot_overlay(ui.ctx());
        self.render_browse_file_prompt(ui.ctx(), details_rect);
    }

}
