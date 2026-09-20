use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Label, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

use super::data::{Catalog, Category};

const OVERLAY_OPACITY_MIN: u8 = 50;
const OVERLAY_OPACITY_MAX: u8 = 94;
const CAROUSEL_HEIGHT: f32 = 266.0;
const FOCUSED_CARD_SIZE: Vec2 = Vec2::new(214.0, 246.0);
const NEIGHBOR_CARD_SIZE: Vec2 = Vec2::new(78.0, 188.0);
const CAROUSEL_CARD_GAP: f32 = 12.0;
const CATEGORY_STRIP_HEIGHT: f32 = 78.0;
const CATEGORY_ITEM_HEIGHT: f32 = 64.0;
const CATEGORY_SELECTED_WIDTH: f32 = 146.0;
const CATEGORY_ITEM_WIDTH: f32 = 56.0;
const CATEGORY_GAP: f32 = 6.0;
const ACCENT: Color32 = Color32::from_rgb(196, 91, 52);

// Keep one physical wheel notch from selecting several entries when egui exposes its
// smoothing tail over multiple frames.
const WHEEL_POINTS_PER_STEP: f32 = 50.0;
const WHEEL_IDLE_RESET_SECS: f64 = 0.35;
const WHEEL_STEP_COOLDOWN_SECS: f64 = 0.12;

/// In-memory state for the native costume switcher preview.
pub(super) struct Layouts {
    catalog: Catalog,
    selected_category: usize,
    carousel_focus: usize,
    active_images: Vec<Option<PathBuf>>,
    textures: HashMap<PathBuf, egui::TextureHandle>,
    failed_images: HashSet<PathBuf>,
    reveal_category: bool,
    category_wheel_accum: f32,
    category_wheel_last_event_at: f64,
    category_wheel_last_step_at: f64,
    category_wheel_suppress_until: f64,
    carousel_wheel_accum: f32,
    carousel_wheel_last_event_at: f64,
    carousel_wheel_last_step_at: f64,
    carousel_wheel_suppress_until: f64,
}

impl Layouts {
    pub(super) fn new(catalog: Catalog) -> Self {
        let selected_category = preferred_category(&catalog);
        let carousel_focus = catalog
            .categories
            .get(selected_category)
            .map(active_costume_index)
            .unwrap_or(0);
        let active_images = catalog
            .categories
            .iter()
            .map(active_image)
            .collect::<Vec<_>>();

        Self {
            catalog,
            selected_category,
            carousel_focus,
            active_images,
            textures: HashMap::new(),
            failed_images: HashSet::new(),
            reveal_category: true,
            category_wheel_accum: 0.0,
            category_wheel_last_event_at: f64::NEG_INFINITY,
            category_wheel_last_step_at: f64::NEG_INFINITY,
            category_wheel_suppress_until: f64::NEG_INFINITY,
            carousel_wheel_accum: 0.0,
            carousel_wheel_last_event_at: f64::NEG_INFINITY,
            carousel_wheel_last_step_at: f64::NEG_INFINITY,
            carousel_wheel_suppress_until: f64::NEG_INFINITY,
        }
    }

    /// Draw the centered mod carousel. The parent reserves approximately 560x266 for it.
    pub(super) fn show_carousel(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        if self.catalog.categories.is_empty() {
            self.show_empty_catalog(ui, overlay_opacity);
            return;
        }

        self.clamp_selection();
        let available = ui.available_size();
        let width = available.x.max(1.0);
        let height = available.y.min(CAROUSEL_HEIGHT).max(1.0);
        let (carousel_rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let category_index = self.selected_category;
        let costume_count = self.catalog.categories[category_index].costumes.len();
        if costume_count == 0 {
            ui.painter().text(
                carousel_rect.center(),
                Align2::CENTER_CENTER,
                "No installed mods",
                FontId::proportional(15.0),
                content_gray(235, overlay_opacity),
            );
            return;
        }

        if let Some(direction) = self.consume_carousel_input(ui, carousel_rect, costume_count) {
            self.carousel_focus = next_focus_index(costume_count, self.carousel_focus, direction);
            ui.ctx().request_repaint();
        }

        let focus = self.carousel_focus.min(costume_count - 1);
        let left = if costume_count > 2 {
            Some(previous_focus_index(costume_count, focus))
        } else if costume_count == 2 && focus == 1 {
            Some(0)
        } else {
            None
        };
        let right = if costume_count > 2 {
            Some(next_focus_index(costume_count, focus, 1))
        } else if costume_count == 2 && focus == 0 {
            Some(1)
        } else {
            None
        };

        let focused_rect = Rect::from_center_size(
            egui::pos2(carousel_rect.center().x, carousel_rect.center().y),
            FOCUSED_CARD_SIZE,
        );
        if let Some(index) = left {
            let rect = Rect::from_center_size(
                egui::pos2(
                    focused_rect.min.x - CAROUSEL_CARD_GAP - NEIGHBOR_CARD_SIZE.x * 0.5,
                    carousel_rect.center().y,
                ),
                NEIGHBOR_CARD_SIZE,
            );
            self.show_carousel_card(ui, category_index, index, rect, false, overlay_opacity);
        }
        if let Some(index) = right {
            let rect = Rect::from_center_size(
                egui::pos2(
                    focused_rect.max.x + CAROUSEL_CARD_GAP + NEIGHBOR_CARD_SIZE.x * 0.5,
                    carousel_rect.center().y,
                ),
                NEIGHBOR_CARD_SIZE,
            );
            self.show_carousel_card(ui, category_index, index, rect, false, overlay_opacity);
        }
        self.show_carousel_card(
            ui,
            category_index,
            focus,
            focused_rect,
            true,
            overlay_opacity,
        );
    }

    /// Draw the horizontally scrollable category strip. The parent paints its neutral base.
    pub(super) fn show_category_strip(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        if self.catalog.categories.is_empty() {
            return;
        }

        self.clamp_selection();
        let visible_indices = (0..self.catalog.categories.len()).collect::<Vec<_>>();
        let strip_rect = Rect::from_min_size(
            ui.cursor().min,
            Vec2::new(
                ui.available_width().max(1.0),
                CATEGORY_STRIP_HEIGHT.min(ui.available_height().max(1.0)),
            ),
        );
        if let Some(index) = self.consume_category_wheel(ui, strip_rect, &visible_indices) {
            self.select_category(index);
        }

        egui::ScrollArea::horizontal()
            .id_salt("overlay-preview-category-strip")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(CATEGORY_ITEM_HEIGHT);
                    for &index in &visible_indices {
                        let selected = self.selected_category == index;
                        self.show_category_item(ui, index, selected, overlay_opacity);
                        ui.add_space(CATEGORY_GAP);
                    }
                });
            });
    }

    fn show_empty_catalog(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        ui.vertical_centered(|ui| {
            ui.add_space(68.0);
            ui.label(
                RichText::new("No installed mods")
                    .size(16.0)
                    .strong()
                    .color(content_gray(255, overlay_opacity)),
            );
            ui.add_space(5.0);
            ui.add(
                Label::new(
                    RichText::new(
                        self.catalog
                            .note
                            .as_deref()
                            .unwrap_or("Install a mod to see it here."),
                    )
                    .size(12.0)
                    .color(content_gray(160, overlay_opacity)),
                )
                .wrap(),
            );
        });
    }

    fn clamp_selection(&mut self) {
        if self.catalog.categories.is_empty() {
            self.selected_category = 0;
            self.carousel_focus = 0;
            return;
        }
        self.selected_category = self
            .selected_category
            .min(self.catalog.categories.len().saturating_sub(1));
        let count = self.catalog.categories[self.selected_category]
            .costumes
            .len();
        self.carousel_focus = self.carousel_focus.min(count.saturating_sub(1));
    }

    fn select_category(&mut self, index: usize) {
        if index >= self.catalog.categories.len() || self.selected_category == index {
            return;
        }
        self.selected_category = index;
        self.carousel_focus = active_costume_index(&self.catalog.categories[index]);
        self.reveal_category = true;
    }

    fn show_category_item(
        &mut self,
        ui: &mut Ui,
        index: usize,
        selected: bool,
        overlay_opacity: u8,
    ) {
        let category_name = self.catalog.categories[index].name.clone();
        let name = character_name(&category_name).to_owned();
        let image = self.catalog.categories[index]
            .image
            .clone()
            .or_else(|| self.active_images[index].clone());
        let width = if selected {
            CATEGORY_SELECTED_WIDTH
        } else {
            CATEGORY_ITEM_WIDTH
        };
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, CATEGORY_ITEM_HEIGHT), Sense::click());
        if response.clicked() {
            self.select_category(index);
        }
        if selected && self.reveal_category {
            ui.scroll_to_rect(rect, Some(Align::Center));
            self.reveal_category = false;
        }

        let fill = if selected {
            rgba_alpha(65, 65, 65, 118, overlay_opacity)
        } else if response.hovered() {
            white_alpha(15, overlay_opacity)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);

        if selected {
            let image_rect = Rect::from_min_size(
                rect.min + Vec2::new(5.0, 5.0),
                Vec2::splat(CATEGORY_ITEM_HEIGHT - 10.0),
            );
            let texture = self.texture_for(ui, image.as_deref());
            paint_thumbnail_tinted(
                ui,
                image_rect,
                texture.as_ref(),
                true,
                false,
                image_alpha(overlay_opacity),
            );
            let text_rect = Rect::from_min_max(
                egui::pos2(image_rect.max.x + 8.0, rect.min.y + 9.0),
                egui::pos2(rect.max.x - 6.0, rect.max.y - 9.0),
            );
            paint_text(
                ui,
                text_rect,
                &name,
                FontId::proportional(12.0),
                content_gray(242, overlay_opacity),
                2,
            );
            ui.painter().rect_stroke(
                rect.shrink(0.5),
                CornerRadius::same(4),
                Stroke::new(1.0, content_rgba(223, 119, 73, 180, overlay_opacity)),
                StrokeKind::Inside,
            );
        } else {
            let image_rect = Rect::from_min_size(
                egui::pos2(rect.center().x - 14.0, rect.min.y + 5.0),
                Vec2::splat(28.0),
            );
            let texture = self.texture_for(ui, image.as_deref());
            paint_thumbnail_tinted(
                ui,
                image_rect,
                texture.as_ref(),
                true,
                false,
                image_alpha(overlay_opacity),
            );
            paint_text(
                ui,
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 3.0, rect.min.y + 37.0),
                    egui::pos2(rect.max.x - 3.0, rect.max.y - 3.0),
                ),
                &name,
                FontId::proportional(9.0),
                content_gray(220, overlay_opacity),
                2,
            );
        }
        response.on_hover_text(category_name);
    }

    fn show_carousel_card(
        &mut self,
        ui: &mut Ui,
        category_index: usize,
        costume_index: usize,
        rect: Rect,
        focused: bool,
        overlay_opacity: u8,
    ) {
        let (name, image, active) = {
            let costume = &self.catalog.categories[category_index].costumes[costume_index];
            (costume.name.clone(), costume.image.clone(), costume.active)
        };
        let response = ui
            .interact(
                rect,
                ui.id().with((
                    "overlay-preview-carousel-card",
                    category_index,
                    costume_index,
                )),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            self.carousel_focus = costume_index;
            select_costume(&mut self.catalog.categories[category_index], costume_index);
            let new_active_image = active_image(&self.catalog.categories[category_index]);
            self.active_images[category_index] = new_active_image;
            ui.ctx().request_repaint();
        }

        let hovered = response.hovered();
        let border = if active {
            content_color(ACCENT, overlay_opacity)
        } else if focused || hovered {
            content_gray(178, overlay_opacity)
        } else {
            content_gray(82, overlay_opacity)
        };
        let image_rect = rect.shrink(1.0);
        let texture = self.texture_for(ui, image.as_deref());
        paint_thumbnail_tinted(
            ui,
            image_rect,
            texture.as_ref(),
            true,
            false,
            image_alpha(overlay_opacity),
        );
        paint_bottom_gradient(ui, image_rect, overlay_opacity);
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            CornerRadius::ZERO,
            Stroke::new(if active || focused { 1.5 } else { 1.0 }, border),
            StrokeKind::Inside,
        );

        if active {
            let badge_rect = Rect::from_min_size(rect.min + Vec2::new(8.0, 8.0), Vec2::splat(21.0));
            ui.painter().rect_filled(
                badge_rect,
                CornerRadius::ZERO,
                Color32::from_black_alpha(chrome_alpha(185, overlay_opacity)),
            );
            ui.painter().text(
                badge_rect.center(),
                Align2::CENTER_CENTER,
                char::from(lucide_icons::Icon::Check).to_string(),
                FontId::new(14.0, egui::FontFamily::Name("preview-icons".into())),
                content_color(ACCENT, overlay_opacity),
            );
        }

        let name_rect = Rect::from_min_max(
            egui::pos2(
                image_rect.min.x + 8.0,
                image_rect.max.y - if focused { 48.0 } else { 39.0 },
            ),
            egui::pos2(image_rect.max.x - 8.0, image_rect.max.y - 7.0),
        );
        paint_card_name(ui, name_rect, &name, overlay_opacity, focused);
        response.on_hover_text(name);
    }

    fn consume_carousel_input(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        costume_count: usize,
    ) -> Option<i32> {
        if costume_count == 0 {
            return None;
        }
        let hovered = ui.rect_contains_pointer(rect);
        let now = ui.input(|input| input.time);
        let mut direction = None;
        ui.input_mut(|input| {
            let mut consumed_wheel = false;
            input.events.retain(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } if hovered && !modifiers.ctrl && !modifiers.command => {
                    consumed_wheel = true;
                    if *phase == egui::TouchPhase::Move {
                        self.carousel_wheel_accum += wheel_units(*unit, delta.y);
                    }
                    false
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if navigation_modifiers(*modifiers) => {
                    match key {
                        egui::Key::ArrowLeft | egui::Key::ArrowUp => direction = Some(-1),
                        egui::Key::ArrowRight | egui::Key::ArrowDown => direction = Some(1),
                        egui::Key::Enter => direction = Some(0),
                        _ => return true,
                    }
                    false
                }
                _ => true,
            });
            input.raw.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::MouseWheel { modifiers, .. }
                        if hovered && !modifiers.ctrl && !modifiers.command
                )
            });
            if consumed_wheel || now < self.carousel_wheel_suppress_until {
                input.smooth_scroll_delta = Vec2::ZERO;
            }
        });

        if self.carousel_wheel_accum != 0.0 {
            self.carousel_wheel_last_event_at = now;
            self.carousel_wheel_suppress_until = now + WHEEL_IDLE_RESET_SECS;
            if now - self.carousel_wheel_last_step_at >= WHEEL_STEP_COOLDOWN_SECS
                && self.carousel_wheel_accum.abs() >= 1.0
            {
                direction = Some(if self.carousel_wheel_accum > 0.0 {
                    -1
                } else {
                    1
                });
                self.carousel_wheel_accum = 0.0;
                self.carousel_wheel_last_step_at = now;
            }
        } else if now - self.carousel_wheel_last_event_at > WHEEL_IDLE_RESET_SECS {
            self.carousel_wheel_accum = 0.0;
        }

        if direction == Some(0) {
            self.activate_focused_costume();
            None
        } else {
            direction
        }
    }

    fn consume_category_wheel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        visible_indices: &[usize],
    ) -> Option<usize> {
        if visible_indices.is_empty() || !ui.rect_contains_pointer(rect) {
            return None;
        }
        let now = ui.input(|input| input.time);
        let mut consumed_wheel = false;
        ui.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } = event
                else {
                    return true;
                };
                if modifiers.ctrl || modifiers.command {
                    return true;
                }
                consumed_wheel = true;
                if *phase == egui::TouchPhase::Move {
                    self.category_wheel_accum += wheel_units(*unit, delta.y);
                }
                false
            });
            input.raw.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::MouseWheel { modifiers, .. }
                        if !modifiers.ctrl && !modifiers.command
                )
            });
            if consumed_wheel || now < self.category_wheel_suppress_until {
                input.smooth_scroll_delta = Vec2::ZERO;
            }
        });

        if consumed_wheel {
            self.category_wheel_last_event_at = now;
            self.category_wheel_suppress_until = now + WHEEL_IDLE_RESET_SECS;
        }
        if now - self.category_wheel_last_event_at > WHEEL_IDLE_RESET_SECS {
            self.category_wheel_accum = 0.0;
        }
        if self.category_wheel_accum.abs() < 1.0
            || now - self.category_wheel_last_step_at < WHEEL_STEP_COOLDOWN_SECS
        {
            if consumed_wheel {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        WHEEL_STEP_COOLDOWN_SECS,
                    ));
            }
            return None;
        }
        let direction = if self.category_wheel_accum > 0.0 {
            -1
        } else {
            1
        };
        self.category_wheel_accum = 0.0;
        self.category_wheel_last_step_at = now;
        next_visible_category(visible_indices, self.selected_category, direction)
    }

    fn activate_focused_costume(&mut self) {
        let category_index = self.selected_category;
        let costume_index = self.carousel_focus;
        if let Some(category) = self.catalog.categories.get_mut(category_index) {
            select_costume(category, costume_index);
            let new_active_image = active_image(category);
            self.active_images[category_index] = new_active_image;
        }
    }

    fn texture_for(&mut self, ui: &Ui, path: Option<&Path>) -> Option<egui::TextureHandle> {
        let path = path?;
        if let Some(texture) = self.textures.get(path) {
            return Some(texture.clone());
        }
        if self.failed_images.contains(path) {
            return None;
        }

        let image = match image::open(path) {
            Ok(image) => image.thumbnail(640, 640).to_rgba8(),
            Err(_) => {
                self.failed_images.insert(path.to_path_buf());
                return None;
            }
        };
        let size = [image.width() as usize, image.height() as usize];
        if size[0] == 0 || size[1] == 0 {
            self.failed_images.insert(path.to_path_buf());
            return None;
        }
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = ui.ctx().load_texture(
            format!("overlay-preview-image:{}", path.display()),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.textures.insert(path.to_path_buf(), texture.clone());
        Some(texture)
    }
}

fn navigation_modifiers(modifiers: egui::Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.command && !modifiers.shift
}

fn wheel_units(unit: egui::MouseWheelUnit, delta_y: f32) -> f32 {
    if delta_y == 0.0 {
        return 0.0;
    }
    match unit {
        egui::MouseWheelUnit::Point => (delta_y / WHEEL_POINTS_PER_STEP).clamp(-1.0, 1.0),
        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta_y.signum(),
    }
}

fn next_visible_category(
    visible_indices: &[usize],
    selected_category: usize,
    direction: i32,
) -> Option<usize> {
    if visible_indices.is_empty() {
        return None;
    }
    let current = visible_indices
        .iter()
        .position(|&index| index == selected_category);
    let target = match (current, direction.signum()) {
        (Some(position), -1) => position.checked_sub(1),
        (Some(position), 1) => (position + 1 < visible_indices.len()).then_some(position + 1),
        (None, -1) => visible_indices
            .iter()
            .rposition(|&index| index < selected_category)
            .or_else(|| Some(0)),
        (None, 1) => visible_indices
            .iter()
            .position(|&index| index > selected_category)
            .or_else(|| Some(visible_indices.len() - 1)),
        _ => None,
    }?;
    visible_indices.get(target).copied()
}

fn next_focus_index(len: usize, current: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let current = current.min(len - 1);
    match direction.signum() {
        -1 if current == 0 => len - 1,
        -1 => current - 1,
        1 => (current + 1) % len,
        _ => current,
    }
}

fn previous_focus_index(len: usize, current: usize) -> usize {
    next_focus_index(len, current, -1)
}

fn active_costume_index(category: &Category) -> usize {
    category
        .costumes
        .iter()
        .position(|costume| costume.active)
        .unwrap_or(0)
}

fn active_image(category: &Category) -> Option<PathBuf> {
    category
        .costumes
        .iter()
        .find(|costume| costume.active)
        .and_then(|costume| costume.image.clone())
        .or_else(|| category.image.clone())
}

fn preferred_category(catalog: &Catalog) -> usize {
    catalog
        .categories
        .iter()
        .position(|category| {
            category.costumes.len() >= 2
                && category
                    .costumes
                    .iter()
                    .filter(|costume| costume.active)
                    .count()
                    == 1
                && category
                    .costumes
                    .iter()
                    .any(|costume| costume.image.is_some())
        })
        .or_else(|| {
            catalog.categories.iter().position(|category| {
                category
                    .costumes
                    .iter()
                    .filter(|costume| costume.image.is_some())
                    .count()
                    >= 2
            })
        })
        .unwrap_or(0)
}

fn character_name(name: &str) -> &str {
    name.strip_prefix("Operators: ").unwrap_or(name)
}

fn paint_text(ui: &Ui, rect: Rect, text: &str, font: FontId, color: Color32, max_rows: usize) {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, rect.width());
    job.wrap.max_rows = max_rows;
    job.wrap.break_anywhere = false;
    let galley = ui.painter().layout_job(job);
    let position = egui::pos2(rect.min.x, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(position, galley, color);
}

fn clean_display_name(name: &str) -> String {
    static VERSION_SUFFIX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)(?:\s+v(?:ersion)?\s*\d+(?:\.\d+)*|\s*\d+(?:\.\d+)+)\s*$")
            .expect("valid preview version suffix")
    });
    let readable = name
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = VERSION_SUFFIX.replace(&readable, "");
    if cleaned.trim().is_empty() {
        readable
    } else {
        cleaned.trim().to_owned()
    }
}

fn opacity_unit(overlay_opacity: u8) -> f32 {
    f32::from(overlay_opacity.clamp(OVERLAY_OPACITY_MIN, OVERLAY_OPACITY_MAX)) / 100.0
}

fn image_alpha(overlay_opacity: u8) -> u8 {
    (opacity_unit(overlay_opacity) * 255.0).round() as u8
}

fn chrome_opacity_unit(overlay_opacity: u8) -> f32 {
    opacity_unit(overlay_opacity).powf(1.3)
}

fn content_opacity_unit(overlay_opacity: u8) -> f32 {
    0.90 + 0.10 * (opacity_unit(overlay_opacity) - 0.50) / 0.44
}

fn chrome_alpha(base_alpha: u8, overlay_opacity: u8) -> u8 {
    (f32::from(base_alpha) * chrome_opacity_unit(overlay_opacity)).round() as u8
}

fn content_alpha(base_alpha: u8, overlay_opacity: u8) -> u8 {
    (f32::from(base_alpha) * content_opacity_unit(overlay_opacity)).round() as u8
}

fn chrome_alpha_from_image_alpha(base_alpha: u8, image_alpha: u8) -> u8 {
    let opacity = f32::from(image_alpha) / 255.0;
    (f32::from(base_alpha) * opacity.powf(1.3)).round() as u8
}

fn content_gray(value: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(value, value, value, content_alpha(255, overlay_opacity))
}

fn chrome_gray_from_image_alpha(value: u8, image_alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        value,
        value,
        value,
        chrome_alpha_from_image_alpha(255, image_alpha),
    )
}

fn content_color(color: Color32, overlay_opacity: u8) -> Color32 {
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, content_alpha(a, overlay_opacity))
}

fn white_alpha(base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_white_alpha(chrome_alpha(base_alpha, overlay_opacity))
}

fn rgba_alpha(r: u8, g: u8, b: u8, base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, chrome_alpha(base_alpha, overlay_opacity))
}

fn content_black_alpha(base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_black_alpha(content_alpha(base_alpha, overlay_opacity))
}

fn content_rgba(r: u8, g: u8, b: u8, base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, content_alpha(base_alpha, overlay_opacity))
}

fn paint_thumbnail_tinted(
    ui: &mut Ui,
    rect: Rect,
    texture: Option<&egui::TextureHandle>,
    crop: bool,
    show_missing_text: bool,
    tint_alpha: u8,
) {
    let Some(texture) = texture else {
        ui.painter().rect_filled(
            rect,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(
                65,
                65,
                65,
                chrome_alpha_from_image_alpha(110, tint_alpha),
            ),
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            char::from(lucide_icons::Icon::Image).to_string(),
            FontId::new(18.0, egui::FontFamily::Name("preview-icons".into())),
            chrome_gray_from_image_alpha(145, tint_alpha),
        );
        if show_missing_text {
            ui.painter().text(
                egui::pos2(rect.center().x, rect.center().y + 18.0),
                Align2::CENTER_CENTER,
                "No preview",
                FontId::proportional(9.0),
                chrome_gray_from_image_alpha(139, tint_alpha),
            );
        }
        return;
    };

    let source_size = texture.size_vec2();
    if crop {
        let scale = (rect.width() / source_size.x).max(rect.height() / source_size.y);
        let uv_size = rect.size() / (source_size * scale);
        // Portrait covers tend to place faces above center; keep that region visible.
        let center_y = uv_size.y * 0.5 + (1.0 - uv_size.y) * 0.08;
        let uv = Rect::from_center_size(egui::pos2(0.5, center_y), uv_size);
        egui::Image::from_texture(texture)
            .uv(uv)
            .tint(Color32::from_white_alpha(tint_alpha))
            .corner_radius(CornerRadius::ZERO)
            .paint_at(ui, rect);
        return;
    }
    paint_fitted_image_tinted(ui, rect, Some(texture), tint_alpha);
}

fn paint_fitted_image_tinted(
    ui: &Ui,
    rect: Rect,
    texture: Option<&egui::TextureHandle>,
    tint_alpha: u8,
) {
    let Some(texture) = texture else {
        return;
    };
    let source_size = texture.size_vec2();
    let scale = (rect.width() / source_size.x).min(rect.height() / source_size.y);
    let fitted = source_size * scale;
    let image_rect = Rect::from_center_size(rect.center(), fitted);
    egui::Image::from_texture(texture)
        .tint(Color32::from_white_alpha(tint_alpha))
        .corner_radius(CornerRadius::ZERO)
        .paint_at(ui, image_rect);
}

fn paint_bottom_gradient(ui: &mut Ui, rect: Rect, overlay_opacity: u8) {
    const STOPS: [(f32, u8); 5] = [(0.0, 0), (0.28, 10), (0.52, 34), (0.77, 108), (1.0, 218)];
    let gradient_height = rect.height() * 0.43;
    let start_y = rect.max.y - gradient_height;
    let mut mesh = egui::Mesh::default();
    for row in 0..=64 {
        let stop = row as f32 / 64.0;
        let interval = STOPS.windows(2).find(|pair| stop <= pair[1].0).unwrap();
        let blend = (stop - interval[0].0) / (interval[1].0 - interval[0].0);
        let alpha = (f32::from(interval[0].1)
            + blend * (f32::from(interval[1].1) - f32::from(interval[0].1)))
            as u8;
        let y = start_y + gradient_height * stop;
        let base = mesh.vertices.len() as u32;
        let color = content_black_alpha(alpha, overlay_opacity);
        mesh.vertices.push(egui::epaint::Vertex::untextured(
            egui::pos2(rect.min.x, y),
            color,
        ));
        mesh.vertices.push(egui::epaint::Vertex::untextured(
            egui::pos2(rect.max.x, y),
            color,
        ));
        if row > 0 {
            mesh.indices
                .extend_from_slice(&[base - 2, base, base - 1, base - 1, base, base + 1]);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn paint_card_name(ui: &Ui, rect: Rect, name: &str, overlay_opacity: u8, focused: bool) {
    let mut job = egui::text::LayoutJob::simple(
        clean_display_name(name),
        FontId::proportional(if focused { 12.5 } else { 9.5 }),
        content_gray(255, overlay_opacity),
        rect.width(),
    );
    job.wrap.max_rows = if focused { 2 } else { 3 };
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let position = egui::pos2(rect.min.x, rect.max.y - galley.size().y);
    ui.painter()
        .galley(position, galley, content_gray(255, overlay_opacity));
}

/// Select a costume according to the switcher's exclusivity rule.
fn select_costume(category: &mut Category, index: usize) {
    if index >= category.costumes.len() {
        return;
    }

    let active_count = category
        .costumes
        .iter()
        .filter(|costume| costume.active)
        .count();
    if active_count <= 1 {
        for (costume_index, costume) in category.costumes.iter_mut().enumerate() {
            costume.active = costume_index == index;
        }
    } else {
        category.costumes[index].active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::super::data::Costume;
    use super::*;

    #[test]
    fn display_names_remove_versions_but_preserve_meaningful_numbers() {
        assert_eq!(clean_display_name("Dawn1.8"), "Dawn");
        assert_eq!(clean_display_name("Summer_Outfit_v2"), "Summer Outfit");
        assert_eq!(
            clean_display_name("Summer 2024 Recolor"),
            "Summer 2024 Recolor"
        );
        assert_eq!(clean_display_name("Outfit 2 Blue"), "Outfit 2 Blue");
        assert_eq!(clean_display_name("2B"), "2B");
    }

    #[test]
    fn artwork_opacity_uses_the_clamped_slider_directly() {
        assert_eq!(image_alpha(40), 128);
        assert_eq!(image_alpha(50), 128);
        assert_eq!(image_alpha(55), 140);
        assert_eq!(image_alpha(70), 179);
        assert_eq!(image_alpha(90), 230);
        assert_eq!(image_alpha(94), 240);
        assert_eq!(image_alpha(100), 240);
    }

    #[test]
    fn wheel_units_clamp_each_line_event_to_one_step() {
        assert_eq!(wheel_units(egui::MouseWheelUnit::Line, -4.0), -1.0);
        assert_eq!(wheel_units(egui::MouseWheelUnit::Line, 3.0), 1.0);
        assert_eq!(wheel_units(egui::MouseWheelUnit::Point, -25.0), -0.5);
    }

    #[test]
    fn wheel_selection_skips_filtered_categories_and_clamps_at_boundaries() {
        let visible = vec![1, 4, 8];
        assert_eq!(next_visible_category(&visible, 4, 1), Some(8));
        assert_eq!(next_visible_category(&visible, 4, -1), Some(1));
        assert_eq!(next_visible_category(&visible, 8, 1), None);
        assert_eq!(next_visible_category(&visible, 1, -1), None);
        assert_eq!(next_visible_category(&visible, 3, 1), Some(4));
        assert_eq!(next_visible_category(&visible, 3, -1), Some(1));
    }

    #[test]
    fn carousel_focus_wraps_without_duplicate_neighbors() {
        assert_eq!(next_focus_index(3, 0, -1), 2);
        assert_eq!(next_focus_index(3, 2, 1), 0);
        assert_eq!(next_focus_index(1, 0, -1), 0);
        assert_eq!(next_focus_index(2, 0, 1), 1);
        assert_eq!(next_focus_index(2, 1, -1), 0);
    }

    fn category(active: &[bool]) -> Category {
        Category {
            name: "Ardelia".into(),
            image: None,
            costumes: active
                .iter()
                .enumerate()
                .map(|(index, &active)| Costume {
                    name: format!("Costume {index}"),
                    image: None,
                    active,
                })
                .collect(),
        }
    }

    #[test]
    fn selecting_with_exactly_one_active_switches_exclusively() {
        let mut character = category(&[true, false, false]);
        select_costume(&mut character, 2);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![false, false, true]
        );
    }

    #[test]
    fn selecting_with_no_active_costumes_activates_only_the_clicked_costume() {
        let mut character = category(&[false, false, false]);
        select_costume(&mut character, 1);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![false, true, false]
        );
    }

    #[test]
    fn selecting_with_multiple_active_costumes_preserves_the_other_active_costumes() {
        let mut character = category(&[true, true, false]);
        select_costume(&mut character, 2);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![true, true, true]
        );
    }
}
