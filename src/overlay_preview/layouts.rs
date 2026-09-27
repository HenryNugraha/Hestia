use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
};

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Label, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

use super::data::{Catalog, Category};
use super::thumbnails::ThumbnailCache;
use crate::overlay_protocol::Selection;

const OVERLAY_OPACITY_MIN: u8 = 50;
const OVERLAY_OPACITY_MAX: u8 = 94;
const CAROUSEL_HEIGHT: f32 = 266.0;
const FOCUSED_CARD_SIZE: Vec2 = Vec2::new(214.0, 246.0);
const NEIGHBOR_CARD_SIZE: Vec2 = Vec2::new(133.0, 153.0);
const CAROUSEL_CARD_GAP: f32 = 12.0;
const CATEGORY_STRIP_HEIGHT: f32 = 78.0;
const CATEGORY_ITEM_HEIGHT: f32 = 64.0;
const CATEGORY_ITEM_WIDTH: f32 = 64.0;
const CATEGORY_NAV_WIDTH: f32 = 40.0;
const CATEGORY_GAP: f32 = 6.0;
const ACCENT: Color32 = Color32::from_rgb(196, 91, 52);
const CAROUSEL_TRANSITION_SECS: f64 = 0.14;
const ACTIVE_FEEDBACK_SECS: f64 = 0.15;
const CATEGORY_SPRITE_SIZE: f32 = 30.0;
const CATEGORY_SELECTED_SPRITE_SIZE: f32 = 40.0;
const CARD_TITLE_BOTTOM_PADDING: f32 = 8.0;

// Keep one physical wheel notch from selecting several entries when egui exposes its
// smoothing tail over multiple frames.
const WHEEL_POINTS_PER_STEP: f32 = 50.0;
const WHEEL_IDLE_RESET_SECS: f64 = 0.35;
const WHEEL_STEP_COOLDOWN_SECS: f64 = 0.12;
const TOOLTIP_SUPPRESS_SECS: f64 = 0.50;
const REVEAL_INTERACTION_THRESHOLD: f32 = 0.92;
const REVEAL_OFFSET: f32 = 10.0;

fn tooltip_suppress_id() -> egui::Id {
    egui::Id::new("overlay-preview-tooltip-suppress-until")
}

/// Add a delayed tooltip unless a scroll/collapse has just dismissed transient UI.
/// egui still applies its normal still-pointer delay on top of this guard.
pub(super) fn delayed_tooltip(
    response: egui::Response,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let now = response.ctx.input(|input| input.time);
    let suppressed_until = response.ctx.data(|data| {
        data.get_temp::<f64>(tooltip_suppress_id())
            .unwrap_or(f64::NEG_INFINITY)
    });
    if now >= suppressed_until {
        response.on_hover_text(text)
    } else {
        response
    }
}

/// Suppress transient tooltips for a short settling period after a scroll/collapse.
pub(super) fn suppress_tooltips(ctx: &egui::Context) {
    let until = ctx.input(|input| input.time) + TOOLTIP_SUPPRESS_SECS;
    ctx.data_mut(|data| data.insert_temp(tooltip_suppress_id(), until));
}

#[derive(Clone)]
struct CarouselTransition {
    from: Vec<CarouselCardPlacement>,
    started_at: f64,
}

#[derive(Clone, Copy)]
struct CarouselCardPlacement {
    index: usize,
    rect: Rect,
    focused: bool,
    /// Fixed visual slot: left, center, or right. This is carried through a
    /// transition instead of being inferred from an interpolated rectangle.
    slot: usize,
}

/// The card that won the pointer press. Keep its original rectangle until release so a
/// carousel transition cannot make a click activate a different card underneath the cursor.
#[derive(Clone)]
struct CarouselPointerPress {
    category_index: usize,
    costume_index: usize,
    rect: Rect,
    placements: Vec<CarouselCardPlacement>,
}

#[derive(Clone, Copy)]
struct HeldPreview {
    category_index: usize,
    costume_index: usize,
}

enum PendingCommand {
    Mod(i32),
    Category(i32),
    Action(ModAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModAction {
    Exclusive,
    Toggle,
}

/// The row that A/D and the side arrows move in. The overlay opens on the
/// categories row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Row {
    Mods,
    #[default]
    Categories,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ShortcutAvailability {
    pub categories: bool,
    pub mods: bool,
    pub exclusive: bool,
    pub toggle: bool,
}

/// What a search shows.  A category whose name matches shows all its mods,
/// any other category shows only the mods that match, and categories with no
/// match are hidden.  An empty search shows everything.
struct Filter {
    /// The search as typed.
    query: String,
    /// Visible categories, in catalog order.
    categories: Vec<usize>,
    /// Each category's visible mods, in catalog order.  Indexed like the
    /// catalog, so a hidden category has an empty list.
    mods: Vec<Vec<usize>>,
}

impl Filter {
    fn new(catalog: &Catalog, query: &str) -> Self {
        let needle = search_key(query);
        let mut categories = Vec::new();
        let mut mods = Vec::with_capacity(catalog.categories.len());
        for (index, category) in catalog.categories.iter().enumerate() {
            let all =
                needle.is_empty() || search_key(character_name(&category.name)).contains(&needle);
            let visible: Vec<usize> = category
                .costumes
                .iter()
                .enumerate()
                .filter(|(_, costume)| all || search_key(&costume.name).contains(&needle))
                .map(|(costume_index, _)| costume_index)
                .collect();
            if all || !visible.is_empty() {
                categories.push(index);
            }
            mods.push(visible);
        }
        Self {
            query: query.to_owned(),
            categories,
            mods,
        }
    }
}

/// Text as a search compares it: underscores read as spaces, runs of spaces
/// count as one, and case does not matter.  A mod's raw name holds its
/// displayed name, so matching the raw name also matches what is shown.
fn search_key(text: &str) -> String {
    text.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// In-memory state for the native costume switcher preview.
pub(super) struct Layouts {
    catalog: Catalog,
    selected_category: usize,
    carousel_focus: usize,
    carousel_focus_by_category: Vec<usize>,
    active_images: Vec<Option<PathBuf>>,
    thumbnails: ThumbnailCache,
    reveal_category: bool,
    category_wheel_accum: f32,
    category_wheel_last_event_at: f64,
    category_wheel_last_step_at: f64,
    category_wheel_suppress_until: f64,
    carousel_wheel_accum: f32,
    carousel_wheel_last_event_at: f64,
    carousel_wheel_last_step_at: f64,
    carousel_wheel_suppress_until: f64,
    carousel_transition: Option<CarouselTransition>,
    carousel_pointer_press: Option<CarouselPointerPress>,
    held_preview: Option<HeldPreview>,
    reveal_progress: f32,
    visible_card_rects: Vec<Rect>,
    active_feedback_until: f64,
    active_feedback_costume: Option<usize>,
    boundary_feedback_until: f64,
    boundary_feedback_edge: Option<i32>,
    pending_commands: VecDeque<PendingCommand>,
    row: Row,
    filter: Filter,
    /// The live overlay can't change mods yet, so clicks and keys that would
    /// switch them do nothing.
    read_only: bool,
}

impl Layouts {
    pub(super) fn new(catalog: Catalog) -> Self {
        // Start at the leftmost category.
        let selected_category = 0;
        let carousel_focus = catalog
            .categories
            .get(selected_category)
            .map(active_costume_index)
            .unwrap_or(0);
        let carousel_focus_by_category = catalog
            .categories
            .iter()
            .map(active_costume_index)
            .collect::<Vec<_>>();
        let active_images = catalog
            .categories
            .iter()
            .map(active_image)
            .collect::<Vec<_>>();
        let filter = Filter::new(&catalog, "");
        let mut thumbnails = ThumbnailCache::new();
        thumbnails.set_censored(censored_images(&catalog));

        Self {
            catalog,
            selected_category,
            carousel_focus,
            carousel_focus_by_category,
            active_images,
            thumbnails,
            reveal_category: true,
            category_wheel_accum: 0.0,
            category_wheel_last_event_at: f64::NEG_INFINITY,
            category_wheel_last_step_at: f64::NEG_INFINITY,
            category_wheel_suppress_until: f64::NEG_INFINITY,
            carousel_wheel_accum: 0.0,
            carousel_wheel_last_event_at: f64::NEG_INFINITY,
            carousel_wheel_last_step_at: f64::NEG_INFINITY,
            carousel_wheel_suppress_until: f64::NEG_INFINITY,
            carousel_transition: None,
            carousel_pointer_press: None,
            held_preview: None,
            reveal_progress: 1.0,
            visible_card_rects: Vec::new(),
            active_feedback_until: f64::NEG_INFINITY,
            active_feedback_costume: None,
            boundary_feedback_until: f64::NEG_INFINITY,
            boundary_feedback_edge: None,
            pending_commands: VecDeque::new(),
            row: Row::default(),
            filter,
            read_only: false,
        }
    }

    pub(super) fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// Where the overlay is, by ids: the selected category, and each
    /// category's focused mod where it isn't the one a fresh start would pick.
    pub(super) fn selection(&self) -> Selection {
        let mods = self
            .catalog
            .categories
            .iter()
            .enumerate()
            .filter_map(|(index, category)| {
                let focus = if index == self.selected_category {
                    self.carousel_focus
                } else {
                    *self.carousel_focus_by_category.get(index)?
                };
                (focus != active_costume_index(category)).then_some((
                    category.id.clone(),
                    category.costumes.get(focus)?.id.clone(),
                ))
            })
            .collect();
        Selection {
            category_id: self
                .catalog
                .categories
                .get(self.selected_category)
                .map(|category| category.id.clone()),
            mods,
        }
    }

    /// Go back to where `selection` left the overlay.  Without its category,
    /// the overlay starts at the leftmost one.
    pub(super) fn restore_selection(&mut self, selection: &Selection) {
        self.apply_selection(selection, 0);
    }

    /// Take a new library from Hestia and stay on the same category and mods,
    /// by id.  If the selected category is gone, the selection stays at the
    /// same place in the rail.  A category left on its active mod follows the
    /// active mod, as it would on a fresh start.
    pub(super) fn replace_catalog(&mut self, catalog: Catalog) {
        let selection = self.selection();
        let previous_index = self.selected_category;
        let query = std::mem::take(&mut self.filter.query);
        self.catalog = catalog;
        self.thumbnails.set_censored(censored_images(&self.catalog));
        self.carousel_focus_by_category = self
            .catalog
            .categories
            .iter()
            .map(active_costume_index)
            .collect();
        self.active_images = self.catalog.categories.iter().map(active_image).collect();
        self.filter = Filter::new(&self.catalog, &query);
        self.carousel_transition = None;
        self.carousel_pointer_press = None;
        self.held_preview = None;
        self.visible_card_rects.clear();
        self.active_feedback_until = f64::NEG_INFINITY;
        self.active_feedback_costume = None;
        self.boundary_feedback_until = f64::NEG_INFINITY;
        self.boundary_feedback_edge = None;
        self.apply_selection(&selection, previous_index);
    }

    fn apply_selection(&mut self, selection: &Selection, fallback_category: usize) {
        for (index, category) in self.catalog.categories.iter().enumerate() {
            let focus = selection.mods.get(&category.id).and_then(|mod_id| {
                category
                    .costumes
                    .iter()
                    .position(|costume| &costume.id == mod_id)
            });
            if let (Some(focus), Some(slot)) =
                (focus, self.carousel_focus_by_category.get_mut(index))
            {
                *slot = focus;
            }
        }
        let last = self.catalog.categories.len().saturating_sub(1);
        self.selected_category = selection
            .category_id
            .as_ref()
            .and_then(|id| {
                self.catalog
                    .categories
                    .iter()
                    .position(|category| &category.id == id)
            })
            .unwrap_or(fallback_category)
            .min(last);
        self.carousel_focus = self
            .carousel_focus_by_category
            .get(self.selected_category)
            .copied()
            .unwrap_or(0);
        self.clamp_selection();
        if !self.filter.categories.contains(&self.selected_category) {
            if let Some(&first) = self.filter.categories.first() {
                self.selected_category = first;
                self.carousel_focus = self.carousel_focus_by_category[first];
                self.clamp_selection();
            }
        }
        self.carousel_focus = self.visible_focus(self.selected_category, self.carousel_focus);
        if let Some(slot) = self
            .carousel_focus_by_category
            .get_mut(self.selected_category)
        {
            *slot = self.carousel_focus;
        }
        self.reveal_category = true;
    }

    pub(super) fn prepare_thumbnails(&mut self, ctx: &egui::Context) {
        self.thumbnails.poll(ctx);
        let mut paths = Vec::new();
        // Prioritize the current carousel, then its next cards and adjacent
        // characters. The mini strip also calls this to warm the first view.
        if let Some(category) = self.catalog.categories.get(self.selected_category) {
            append_nearby_mod_images(
                &mut paths,
                category,
                self.visible_mods(),
                self.carousel_focus,
                2,
            );
        }
        let visible = &self.filter.categories;
        let selected = visible
            .iter()
            .position(|&index| index == self.selected_category)
            .unwrap_or(0);
        for (position, &index) in visible
            .iter()
            .enumerate()
            .take(selected + 5)
            .skip(selected.saturating_sub(4))
        {
            let category = &self.catalog.categories[index];
            paths.extend(
                category
                    .image
                    .clone()
                    .or_else(|| self.active_images[index].clone()),
            );
            if index != self.selected_category && position.abs_diff(selected) <= 1 {
                append_nearby_mod_images(
                    &mut paths,
                    category,
                    &self.filter.mods[index],
                    active_costume_index(category),
                    1,
                );
            }
        }
        self.thumbnails.prefetch(ctx, paths);
    }

    /// Draw the centered mod carousel. The parent reserves approximately 560x266 for it.
    pub(super) fn show_carousel(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        self.visible_card_rects.clear();
        if self.catalog.categories.is_empty() {
            let empty_rects = self.show_empty_catalog(ui, overlay_opacity);
            self.visible_card_rects.extend(empty_rects);
            return;
        }

        self.clamp_selection();
        let available = ui.available_size();
        let width = available.x.max(1.0);
        let height = available.y.min(CAROUSEL_HEIGHT).max(1.0);
        let (carousel_rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let now = ui.input(|input| input.time);
        self.consume_pending_commands(ui.ctx(), carousel_rect, now);
        self.clamp_selection();
        let category_index = self.selected_category;
        let visible_count = self.visible_mods().len();
        if visible_count == 0 {
            let text = self.empty_carousel_text();
            let font = FontId::proportional(15.0);
            let galley = ui.painter().layout_no_wrap(
                text.to_owned(),
                font.clone(),
                content_gray(235, overlay_opacity),
            );
            ui.painter().text(
                carousel_rect.center(),
                Align2::CENTER_CENTER,
                text,
                font,
                content_gray(235, overlay_opacity),
            );
            self.visible_card_rects
                .push(Rect::from_center_size(carousel_rect.center(), galley.size()).expand(2.0));
            return;
        }

        if self.carousel_pointer_press.is_some()
            && ui.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::PointerGone))
                    || (input.pointer.latest_pos().is_none()
                        && !input.pointer.primary_down()
                        && !input.pointer.primary_released())
            })
        {
            // A lost pointer cannot produce a trustworthy release target. Drop the capture so
            // a later click after Alt/collapse cannot activate a stale card.
            self.cancel_pointer_interaction();
        }
        let mut placements = self.current_carousel_placements(carousel_rect, now);
        let wheel_direction = self.consume_carousel_wheel(ui, carousel_rect, visible_count);
        if let Some(direction) = wheel_direction {
            self.advance_mod_focus(carousel_rect, direction, now);
            placements = self.current_carousel_placements(carousel_rect, now);
            ui.ctx().request_repaint();
        }

        self.update_held_preview(ui, category_index, &placements, visible_count);

        if self.held_preview.is_none() {
            self.capture_carousel_press(ui, category_index, &placements, visible_count);
            if self
                .carousel_pointer_press
                .as_ref()
                .is_some_and(|press| press.category_index == category_index)
                && ui
                    .input(|input| input.pointer.primary_down() || input.pointer.primary_released())
            {
                // Keep the pressed card under the cursor through release. This makes a concurrent
                // repaint or carousel transition harmless to the pending click and its release frame.
                if let Some(press) = &self.carousel_pointer_press {
                    placements = press.placements.clone();
                }
            }
        }

        if self.held_preview.is_none() {
            for placement in &placements {
                // The carousel is a visual slot layout: when the category changes, a
                // different costume can occupy the same left/center/right rectangle. Use
                // that stable slot for egui's interaction id so its widget-rect identity
                // does not change underneath the pointer between passes.
                let reveal = card_reveal_progress(placement, self.reveal_progress, visible_count);
                self.show_carousel_card(
                    ui,
                    category_index,
                    placement.index,
                    placement.rect,
                    placement.focused,
                    placement.slot,
                    reveal,
                    overlay_opacity,
                );
            }
        }
        if self.held_preview.is_none() {
            if let Some(costume_index) =
                self.release_carousel_press(ui, category_index, carousel_rect, now)
            {
                // A click activates in place. The focused card remains focused, so a side-card
                // click never moves the target away from the pointer before the next action.
                self.activate_costume_in_place(category_index, costume_index, now);
                ui.ctx().request_repaint();
            }
        }
        self.request_animation_repaint(ui.ctx(), now);
    }

    /// Set the staged expansion progress used by the parent overlay animation.
    /// The focused card appears first; neighbors are staggered by
    /// `card_reveal_progress`. Invisible cards keep their stable slot IDs but do not
    /// accept activation until they are almost fully revealed.
    pub(super) fn set_reveal(&mut self, progress: f32) {
        self.reveal_progress = progress.clamp(0.0, 1.0);
        if self.reveal_progress <= 0.0 {
            self.visible_card_rects.clear();
            self.held_preview = None;
            self.carousel_pointer_press = None;
        }
    }

    /// Queue a mod navigation command for the next carousel pass, when its current geometry is
    /// available. This keeps keyboard navigation on the same animated path as the wheel.
    pub(super) fn navigate_mod(&mut self, ctx: &egui::Context, direction: i32) {
        if direction == 0 || self.pointer_gesture_active_in_context(ctx) {
            return;
        }
        self.pending_commands
            .push_back(PendingCommand::Mod(direction.signum()));
        ctx.request_repaint();
    }

    /// Navigate categories using the same clamped order as the category wheel.
    pub(super) fn navigate_category(&mut self, ctx: &egui::Context, direction: i32) {
        if direction == 0
            || self.pointer_gesture_active_in_context(ctx)
            || self.catalog.categories.is_empty()
        {
            return;
        }
        self.pending_commands
            .push_back(PendingCommand::Category(direction.signum()));
        suppress_tooltips(ctx);
        ctx.request_repaint();
    }

    /// Queue an explicit action for the focused mod. The command is applied on the next
    /// carousel pass, alongside ordinary activation, so navigation and actions retain FIFO order
    /// and reveal/gesture guards.
    pub(super) fn apply_focused_action(&mut self, ctx: &egui::Context, action: ModAction) {
        if self.pointer_gesture_active_in_context(ctx) {
            return;
        }
        self.pending_commands
            .push_back(PendingCommand::Action(action));
        ctx.request_repaint();
    }

    pub(super) fn row(&self) -> Row {
        self.row
    }

    /// Up to the mods row for a negative direction, down to the categories row
    /// for a positive one.
    pub(super) fn switch_row(&mut self, direction: i32) {
        self.row = match direction.signum() {
            -1 => Row::Mods,
            1 => Row::Categories,
            _ => return,
        };
    }

    pub(super) fn reset_row(&mut self) {
        self.row = Row::default();
    }

    pub(super) fn shortcut_availability(&self) -> ShortcutAvailability {
        let categories = self.filter.categories.len() > 1;
        let Some(category) = self.catalog.categories.get(self.selected_category) else {
            return ShortcutAvailability {
                categories,
                mods: false,
                exclusive: false,
                toggle: false,
            };
        };
        let visible = self.visible_mods();
        let mods = visible.len() > 1;
        let Some(focused) = category
            .costumes
            .get(self.carousel_focus)
            .filter(|_| visible.contains(&self.carousel_focus))
        else {
            return ShortcutAvailability {
                categories,
                mods,
                exclusive: false,
                toggle: false,
            };
        };
        // Exclusive also disables mods the search hides, so count them all.
        let active_count = category
            .costumes
            .iter()
            .filter(|costume| costume.active)
            .count();
        ShortcutAvailability {
            categories,
            mods,
            exclusive: !self.read_only && !(focused.active && active_count == 1),
            toggle: !self.read_only,
        }
    }

    /// Show only what `query` matches.  Keeps the selected category and mod
    /// while the search still shows them, otherwise moves to the first match.
    pub(super) fn set_search(&mut self, query: &str) {
        if self.filter.query == query {
            return;
        }
        self.filter = Filter::new(&self.catalog, query);
        self.reveal_category = true;
        self.carousel_transition = None;
        self.carousel_pointer_press = None;
        self.held_preview = None;
        if self.filter.categories.contains(&self.selected_category) {
            self.carousel_focus = self.visible_focus(self.selected_category, self.carousel_focus);
            if let Some(focus) = self
                .carousel_focus_by_category
                .get_mut(self.selected_category)
            {
                *focus = self.carousel_focus;
            }
        } else if let Some(&first) = self.filter.categories.first() {
            self.select_category(first);
        }
    }

    /// What the carousel says when it has no cards.  A search that matches
    /// nothing leaves the selection on a category it hides.
    fn empty_carousel_text(&self) -> &'static str {
        if self.filter.categories.contains(&self.selected_category) {
            "No installed mods"
        } else {
            "No matches"
        }
    }

    /// The selected category's mods that the search shows.
    fn visible_mods(&self) -> &[usize] {
        self.filter
            .mods
            .get(self.selected_category)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// `focus` when the search shows it in `category`, otherwise the first
    /// mod the search shows there.
    fn visible_focus(&self, category: usize, focus: usize) -> usize {
        match self.filter.mods.get(category) {
            Some(visible) if !visible.is_empty() && !visible.contains(&focus) => visible[0],
            _ => focus,
        }
    }

    /// Return the current painted card/preview bounds for the native click-through mask.
    pub(super) fn visible_card_rects(&self) -> &[Rect] {
        &self.visible_card_rects
    }

    fn pointer_gesture_active(&self) -> bool {
        self.held_preview.is_some() || self.carousel_pointer_press.is_some()
    }

    fn pointer_gesture_active_in_context(&self, ctx: &egui::Context) -> bool {
        self.pointer_gesture_active()
            || ctx.input(|input| input.pointer.any_down() || input.pointer.any_pressed())
    }

    /// Clear press/preview state and defer transient tooltip display after collapse or mode
    /// changes. Active costume state is deliberately untouched.
    pub(super) fn dismiss_transient_ui(&mut self, ctx: &egui::Context) {
        self.carousel_pointer_press = None;
        self.held_preview = None;
        self.visible_card_rects.clear();
        self.pending_commands.clear();
        suppress_tooltips(ctx);
    }

    /// Paint the original image held by the secondary mouse button. The preview contains the
    /// source image without cropping and marks only its actual image bounds.
    pub(super) fn show_held_preview(
        &mut self,
        ui: &mut Ui,
        available_rect: Rect,
        overlay_opacity: u8,
    ) {
        let Some(preview) = self.held_preview else {
            return;
        };
        let Some(costume) = self
            .catalog
            .categories
            .get(preview.category_index)
            .and_then(|category| category.costumes.get(preview.costume_index))
        else {
            self.held_preview = None;
            return;
        };
        let image_path = costume.image.clone();
        let texture = self.texture_for(ui, image_path.as_deref());
        let area = available_rect.shrink(18.0);
        let image_rect = texture
            .as_ref()
            .map(|texture| fitted_image_rect(area, texture.size_vec2()))
            .unwrap_or(area);
        let backdrop = image_rect.expand(10.0).intersect(available_rect);
        self.visible_card_rects.clear();
        self.visible_card_rects.push(backdrop);
        // Keep the game visible through the preview. The margin is only a subtle boundary;
        // filling it would effectively stack a second opaque window behind the image.
        ui.painter().rect_stroke(
            backdrop,
            CornerRadius::ZERO,
            Stroke::new(1.0, content_black_alpha(120, overlay_opacity)),
            StrokeKind::Inside,
        );
        paint_thumbnail_tinted(
            ui,
            image_rect,
            texture.as_ref(),
            false,
            true,
            image_alpha(overlay_opacity),
        );
    }

    fn update_held_preview(
        &mut self,
        ui: &Ui,
        category_index: usize,
        placements: &[CarouselCardPlacement],
        card_count: usize,
    ) {
        let (pressed_pos, released, down) = ui.input(|input| {
            let pressed_pos = input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            });
            (
                pressed_pos,
                input.pointer.secondary_released(),
                input.pointer.secondary_down(),
            )
        });
        if released || !down {
            self.held_preview = None;
        }
        if released || !down {
            return;
        }
        let Some(pointer_pos) = pressed_pos else {
            return;
        };
        let Some(card) = placements.iter().rev().find(|placement| {
            card_reveal_progress(placement, self.reveal_progress, card_count)
                >= REVEAL_INTERACTION_THRESHOLD
                && placement.rect.contains(pointer_pos)
        }) else {
            return;
        };
        self.held_preview = Some(HeldPreview {
            category_index,
            costume_index: card.index,
        });
    }

    fn capture_carousel_press(
        &mut self,
        ui: &Ui,
        category_index: usize,
        placements: &[CarouselCardPlacement],
        card_count: usize,
    ) {
        let Some(pointer_pos) = ui.input(|input| {
            if !input.pointer.primary_pressed() {
                return None;
            }
            input
                .events
                .iter()
                .find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
                .or_else(|| input.pointer.press_origin())
        }) else {
            return;
        };
        let Some(card) = placements.iter().rev().find(|placement| {
            card_reveal_progress(placement, self.reveal_progress, card_count)
                >= REVEAL_INTERACTION_THRESHOLD
                && placement.rect.contains(pointer_pos)
        }) else {
            return;
        };
        self.carousel_pointer_press = Some(CarouselPointerPress {
            category_index,
            costume_index: card.index,
            rect: card.rect,
            placements: placements.to_vec(),
        });
    }

    fn release_carousel_press(
        &mut self,
        ui: &Ui,
        category_index: usize,
        carousel_rect: Rect,
        now: f64,
    ) -> Option<usize> {
        if !ui.input(|input| input.pointer.primary_released()) {
            return None;
        }
        let press = self.carousel_pointer_press.take()?;
        let pointer_pos = ui.input(|input| {
            input
                .pointer
                .interact_pos()
                .or_else(|| input.pointer.latest_pos())
        });
        let can_click = ui.input(|input| input.pointer.could_any_button_be_click());
        // Resume from the exact geometry shown while the button was held. This prevents a
        // long-held press from snapping to a transition's settled destination on release.
        self.begin_carousel_transition(press.placements.clone(), carousel_rect, now);
        valid_carousel_release(&press, category_index, pointer_pos, can_click)
    }

    pub(super) fn cancel_pointer_interaction(&mut self) {
        self.carousel_pointer_press = None;
    }

    fn current_carousel_placements(
        &mut self,
        carousel_rect: Rect,
        now: f64,
    ) -> Vec<CarouselCardPlacement> {
        if let Some(press) = &self.carousel_pointer_press {
            // The pointer target owns the geometry until release, even if the original
            // transition would have completed while the button was held.
            return press.placements.clone();
        }
        let target =
            visible_card_placements(carousel_rect, self.visible_mods(), self.carousel_focus);
        let Some(transition) = self.carousel_transition.clone() else {
            return target;
        };
        let progress = ((now - transition.started_at) / CAROUSEL_TRANSITION_SECS).clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.carousel_transition = None;
            return target;
        }

        let eased = (progress * progress * (3.0 - 2.0 * progress)) as f32;
        let mut current = Vec::with_capacity(target.len());
        for target_card in target {
            let from_card = transition
                .from
                .iter()
                .find(|card| card.index == target_card.index)
                .copied()
                .unwrap_or(target_card);
            current.push(CarouselCardPlacement {
                index: target_card.index,
                rect: lerp_rect(from_card.rect, target_card.rect, eased),
                focused: target_card.focused,
                slot: target_card.slot,
            });
        }
        current
    }

    fn begin_carousel_transition(
        &mut self,
        from: Vec<CarouselCardPlacement>,
        carousel_rect: Rect,
        now: f64,
    ) {
        let to = visible_card_placements(carousel_rect, self.visible_mods(), self.carousel_focus);
        if same_card_geometry(&from, &to) {
            self.carousel_transition = None;
        } else {
            self.carousel_transition = Some(CarouselTransition {
                from,
                started_at: now,
            });
        }
    }

    fn consume_pending_commands(&mut self, ctx: &egui::Context, carousel_rect: Rect, now: f64) {
        while let Some(command) = self.pending_commands.pop_front() {
            if self.pointer_gesture_active_in_context(ctx) {
                continue;
            }
            match command {
                PendingCommand::Category(direction) => {
                    if let Some(index) = next_visible_category(
                        &self.filter.categories,
                        self.selected_category,
                        direction,
                    ) {
                        self.select_category(index);
                    }
                }
                PendingCommand::Mod(direction) => {
                    self.advance_mod_focus(carousel_rect, direction, now);
                }
                PendingCommand::Action(action) => {
                    if self.reveal_progress < REVEAL_INTERACTION_THRESHOLD {
                        self.pending_commands
                            .push_front(PendingCommand::Action(action));
                        break;
                    }
                    self.apply_focused_costume_action(action, now);
                }
            }
        }
    }

    fn advance_mod_focus(&mut self, carousel_rect: Rect, direction: i32, now: f64) {
        if direction == 0 || self.pointer_gesture_active() {
            return;
        }
        let Some(next_focus) =
            next_visible_mod(self.visible_mods(), self.carousel_focus, direction)
        else {
            return;
        };
        let from = self.current_carousel_placements(carousel_rect, now);
        if next_focus == self.carousel_focus {
            self.boundary_feedback_until = now + ACTIVE_FEEDBACK_SECS;
            self.boundary_feedback_edge = Some(direction.signum());
        } else {
            self.boundary_feedback_until = f64::NEG_INFINITY;
            self.boundary_feedback_edge = None;
        }
        self.carousel_focus = next_focus;
        if let Some(focus) = self
            .carousel_focus_by_category
            .get_mut(self.selected_category)
        {
            *focus = next_focus;
        }
        self.begin_carousel_transition(from, carousel_rect, now);
    }

    fn request_animation_repaint(&self, ctx: &egui::Context, now: f64) {
        let transition_remaining = self
            .carousel_transition
            .as_ref()
            .map(|transition| (CAROUSEL_TRANSITION_SECS - (now - transition.started_at)).max(0.0));
        let feedback_remaining = (self.active_feedback_until - now).max(0.0);
        let boundary_remaining = (self.boundary_feedback_until - now).max(0.0);
        let remaining = transition_remaining
            .unwrap_or(0.0)
            .max(feedback_remaining)
            .max(boundary_remaining);
        if remaining > 0.0 {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining.min(0.016)));
        }
    }

    pub(super) fn animation_pending(&self, now: f64) -> bool {
        self.carousel_transition
            .as_ref()
            .is_some_and(|transition| now - transition.started_at < CAROUSEL_TRANSITION_SECS)
            || now < self.active_feedback_until
            || now < self.boundary_feedback_until
    }

    fn activate_costume_in_place(&mut self, category_index: usize, costume_index: usize, now: f64) {
        if self.read_only {
            return;
        }
        if let Some(category) = self.catalog.categories.get_mut(category_index) {
            select_costume(category, costume_index);
            self.active_images[category_index] = active_image(category);
        }
        self.active_feedback_until = now + ACTIVE_FEEDBACK_SECS;
        self.active_feedback_costume = Some(costume_index);
    }

    /// Draw the horizontally scrollable category strip. The parent paints its neutral base.
    pub(super) fn show_category_strip(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        if self.catalog.categories.is_empty() {
            return;
        }

        self.clamp_selection();
        let visible_indices = self.filter.categories.clone();
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

        let rail_rect = strip_rect.shrink2(Vec2::new(12.0, 0.0));
        // Let the category rail run beneath the navigation controls. The controls are
        // painted over the rail below, so reserving side gutters would leave an
        // unnecessary gap before the first and after the last category.
        let scroll_rect = rail_rect;
        let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(scroll_rect));
        egui::ScrollArea::horizontal()
            .id_salt("overlay-preview-category-strip")
            .auto_shrink([false, false])
            .show(&mut content_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(CATEGORY_ITEM_HEIGHT);
                    for &index in &visible_indices {
                        let selected = self.selected_category == index;
                        self.show_category_item(ui, index, selected, overlay_opacity);
                        ui.add_space(CATEGORY_GAP);
                    }
                });
            });
        let left_enabled =
            next_visible_category(&visible_indices, self.selected_category, -1).is_some();
        let right_enabled =
            next_visible_category(&visible_indices, self.selected_category, 1).is_some();
        self.show_category_nav_button(
            ui,
            Rect::from_min_size(
                egui::pos2(strip_rect.min.x + 4.0, rail_rect.min.y + 9.0),
                Vec2::new(CATEGORY_NAV_WIDTH, 28.0),
            ),
            -1,
            'Z',
            left_enabled,
            &visible_indices,
            overlay_opacity,
        );
        self.show_category_nav_button(
            ui,
            Rect::from_min_size(
                egui::pos2(
                    strip_rect.max.x - CATEGORY_NAV_WIDTH - 4.0,
                    rail_rect.min.y + 9.0,
                ),
                Vec2::new(CATEGORY_NAV_WIDTH, 28.0),
            ),
            1,
            'C',
            right_enabled,
            &visible_indices,
            overlay_opacity,
        );
    }

    fn show_category_nav_button(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        direction: i32,
        shortcut: char,
        enabled: bool,
        visible_indices: &[usize],
        overlay_opacity: u8,
    ) {
        // At either end, leave the category beneath the unavailable control unobscured.
        if !enabled {
            return;
        }
        let sense = Sense::click();
        let response = ui
            .interact(
                rect,
                ui.id().with(("overlay-preview-category-nav", direction)),
                sense,
            )
            .on_hover_cursor(if enabled {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });
        let response = delayed_tooltip(
            response,
            if direction < 0 {
                "Previous category"
            } else {
                "Next category"
            },
        );
        let hover = ui.ctx().animate_bool_with_time(
            response.id.with("hover"),
            enabled && response.hovered(),
            0.12,
        );
        let pressed = ui.ctx().animate_bool_with_time(
            response.id.with("pressed"),
            enabled && response.is_pointer_button_down_on(),
            0.06,
        );
        let shade = (24.0 + 14.0 * hover - 12.0 * pressed).round() as u8;
        let alpha = if enabled {
            (224.0 + 12.0 * hover).round() as u8
        } else {
            180
        };
        let button_fill = rgba_alpha(shade, shade, shade, alpha, overlay_opacity);
        let visual_rect = rect;
        ui.painter()
            .rect_filled(visual_rect, CornerRadius::same(8), button_fill);
        ui.painter().rect_stroke(
            visual_rect,
            CornerRadius::same(8),
            Stroke::new(
                1.0,
                rgba_alpha(
                    190,
                    190,
                    190,
                    (56.0 + 24.0 * hover).round() as u8,
                    overlay_opacity,
                ),
            ),
            StrokeKind::Inside,
        );
        if enabled && response.clicked() {
            if let Some(index) =
                next_visible_category(visible_indices, self.selected_category, direction)
            {
                self.select_category(index);
                ui.ctx().request_repaint();
            }
        }

        let emphasis = if enabled { 1.0 } else { 0.38 };
        let arrow_color = scale_color_alpha(
            content_gray((220.0 + 30.0 * hover).round() as u8, overlay_opacity),
            emphasis,
        );
        let shortcut_color = scale_color_alpha(
            content_gray((164.0 + 20.0 * hover).round() as u8, overlay_opacity),
            emphasis,
        );
        let center = visual_rect.center();
        let icon = char::from(if direction < 0 {
            lucide_icons::Icon::ChevronLeft
        } else {
            lucide_icons::Icon::ChevronRight
        })
        .to_string();
        let (icon_pos, shortcut_pos) = if direction < 0 {
            (center.x - 8.0, center.x + 8.0)
        } else {
            (center.x + 8.0, center.x - 8.0)
        };
        ui.painter().text(
            egui::pos2(shortcut_pos, center.y),
            Align2::CENTER_CENTER,
            shortcut.to_string(),
            FontId::proportional(10.0),
            shortcut_color,
        );
        ui.painter().text(
            egui::pos2(icon_pos, center.y),
            Align2::CENTER_CENTER,
            icon,
            FontId::new(16.0, egui::FontFamily::Name("preview-icons".into())),
            arrow_color,
        );
    }

    fn show_empty_catalog(&mut self, ui: &mut Ui, overlay_opacity: u8) -> Vec<Rect> {
        let mut visible_rects = Vec::new();
        ui.vertical_centered(|ui| {
            ui.add_space(68.0);
            let title = ui.label(
                RichText::new("No installed mods")
                    .size(16.0)
                    .strong()
                    .color(content_gray(255, overlay_opacity)),
            );
            visible_rects.push(title.rect.expand(2.0));
            ui.add_space(5.0);
            let note = ui.add(
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
            visible_rects.push(note.rect.expand(2.0));
        });
        visible_rects
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
        if let Some(previous_focus) = self
            .carousel_focus_by_category
            .get_mut(self.selected_category)
        {
            *previous_focus = self.carousel_focus;
        }
        self.selected_category = index;
        self.carousel_focus = self
            .carousel_focus_by_category
            .get(index)
            .copied()
            .unwrap_or_else(|| active_costume_index(&self.catalog.categories[index]));
        self.carousel_focus = self.carousel_focus.min(
            self.catalog.categories[index]
                .costumes
                .len()
                .saturating_sub(1),
        );
        self.carousel_focus = self.visible_focus(index, self.carousel_focus);
        self.reveal_category = true;
        self.carousel_transition = None;
        self.carousel_pointer_press = None;
        self.held_preview = None;
        self.visible_card_rects.clear();
        self.active_feedback_until = f64::NEG_INFINITY;
        self.active_feedback_costume = None;
        self.boundary_feedback_until = f64::NEG_INFINITY;
        self.boundary_feedback_edge = None;
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
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(CATEGORY_ITEM_WIDTH, CATEGORY_ITEM_HEIGHT),
            Sense::click(),
        );
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            self.select_category(index);
            ui.ctx().request_repaint();
        }
        if selected && self.reveal_category {
            ui.scroll_to_rect(rect, Some(Align::Center));
            self.reveal_category = false;
        }
        if !ui.is_rect_visible(rect) {
            return;
        }

        let fill = if selected {
            rgba_alpha(65, 65, 65, 92, overlay_opacity)
        } else if response.hovered() {
            white_alpha(15, overlay_opacity)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
        if selected && self.row == Row::Categories {
            // The keyboard is on this row.  The focused card dims to match.
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(1.5, content_gray(232, overlay_opacity)),
                StrokeKind::Inside,
            );
        }

        if selected {
            let image_rect = Rect::from_min_size(
                egui::pos2(
                    rect.center().x - CATEGORY_SELECTED_SPRITE_SIZE * 0.5,
                    rect.min.y + 2.0,
                ),
                Vec2::splat(CATEGORY_SELECTED_SPRITE_SIZE),
            );
            let texture = self.texture_for(ui, image.as_deref());
            paint_thumbnail_tinted(
                ui,
                image_rect,
                texture.as_ref(),
                false,
                false,
                image_alpha(overlay_opacity),
            );
            let truncated = paint_category_name(
                ui,
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 3.0, rect.min.y + 42.0),
                    egui::pos2(rect.max.x - 3.0, rect.max.y - 3.0),
                ),
                &name,
                FontId::proportional(10.0),
                content_gray(242, overlay_opacity),
            );
            if truncated {
                let _ = delayed_tooltip(response, name.clone());
            }
            ui.painter().rect_filled(
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 10.0, rect.max.y - 2.0),
                    egui::pos2(rect.max.x - 10.0, rect.max.y),
                ),
                CornerRadius::same(1),
                content_rgba(223, 119, 73, 220, overlay_opacity),
            );
        } else {
            let image_rect = Rect::from_min_size(
                egui::pos2(
                    rect.center().x - CATEGORY_SPRITE_SIZE * 0.5,
                    rect.min.y + 8.0,
                ),
                Vec2::splat(CATEGORY_SPRITE_SIZE),
            );
            let texture = self.texture_for(ui, image.as_deref());
            paint_thumbnail_tinted(
                ui,
                image_rect,
                texture.as_ref(),
                false,
                false,
                image_alpha(overlay_opacity),
            );
            let truncated = paint_category_name(
                ui,
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 3.0, rect.min.y + 42.0),
                    egui::pos2(rect.max.x - 3.0, rect.max.y - 3.0),
                ),
                &name,
                FontId::proportional(10.0),
                content_gray(220, overlay_opacity),
            );
            if truncated {
                let _ = delayed_tooltip(response, name.clone());
            }
        }
    }

    fn show_carousel_card(
        &mut self,
        ui: &mut Ui,
        category_index: usize,
        costume_index: usize,
        rect: Rect,
        focused: bool,
        slot: usize,
        reveal: f32,
        overlay_opacity: u8,
    ) {
        let (name, image, active) = {
            let costume = &self.catalog.categories[category_index].costumes[costume_index];
            (costume.name.clone(), costume.image.clone(), costume.active)
        };
        let interactable = reveal >= REVEAL_INTERACTION_THRESHOLD;
        let response = ui
            .interact(
                rect,
                ui.id().with(("overlay-preview-carousel-card-slot", slot)),
                if interactable {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            )
            .on_hover_cursor(if interactable {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });

        let hovered = interactable && response.hovered();
        let now = ui.input(|input| input.time);
        let border = if focused && self.row == Row::Mods {
            content_gray(232, overlay_opacity)
        } else if focused || hovered {
            content_gray(180, overlay_opacity)
        } else if active {
            content_gray(132, overlay_opacity)
        } else {
            content_gray(82, overlay_opacity)
        };
        let visual_rect = rect.translate(Vec2::new(0.0, (1.0 - reveal) * REVEAL_OFFSET));
        if reveal > 0.01 {
            self.visible_card_rects.push(visual_rect);
        }
        let image_rect = visual_rect.shrink(1.0);
        let texture = self.texture_for(ui, image.as_deref());
        let name_rect = card_name_rect(image_rect, focused);
        let (name_size, _) = measure_card_name(ui, name_rect, &name, overlay_opacity, focused);
        paint_thumbnail_tinted(
            ui,
            image_rect,
            texture.as_ref(),
            true,
            false,
            scaled_alpha(image_alpha(overlay_opacity), reveal),
        );
        paint_bottom_gradient(
            ui,
            image_rect,
            overlay_opacity,
            name_size.y + CARD_TITLE_BOTTOM_PADDING + 8.0,
            reveal,
        );
        ui.painter().rect_stroke(
            visual_rect.shrink(0.5),
            CornerRadius::ZERO,
            Stroke::new(
                if active || focused { 1.5 } else { 1.0 },
                scale_color_alpha(border, reveal),
            ),
            StrokeKind::Inside,
        );
        if focused && now < self.boundary_feedback_until && self.boundary_feedback_edge.is_some() {
            let edge_x = if self.boundary_feedback_edge == Some(-1) {
                visual_rect.min.x + 3.0
            } else {
                visual_rect.max.x - 3.0
            };
            ui.painter().line_segment(
                [
                    egui::pos2(edge_x, visual_rect.min.y + 10.0),
                    egui::pos2(edge_x, visual_rect.max.y - 10.0),
                ],
                Stroke::new(1.5, content_gray(180, overlay_opacity)),
            );
        }

        if active {
            let feedback = now < self.active_feedback_until
                && self.active_feedback_costume == Some(costume_index);
            if feedback {
                self.request_animation_repaint(ui.ctx(), now);
            }
            let badge_rect =
                Rect::from_min_size(visual_rect.min + Vec2::new(8.0, 8.0), Vec2::splat(21.0));
            ui.painter().rect_filled(
                badge_rect,
                CornerRadius::ZERO,
                scale_color_alpha(
                    Color32::from_black_alpha(chrome_alpha(
                        if feedback { 215 } else { 185 },
                        overlay_opacity,
                    )),
                    reveal,
                ),
            );
            ui.painter().text(
                badge_rect.center(),
                Align2::CENTER_CENTER,
                char::from(lucide_icons::Icon::Check).to_string(),
                FontId::new(14.0, egui::FontFamily::Name("preview-icons".into())),
                scale_color_alpha(content_color(ACCENT, overlay_opacity), reveal),
            );
        }

        if reveal > 0.01 {
            let truncated =
                paint_card_name_revealed(ui, name_rect, &name, overlay_opacity, focused, reveal);
            if truncated && interactable {
                let _ = delayed_tooltip(response, clean_display_name(&name));
            }
        }
    }

    fn consume_carousel_wheel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        card_count: usize,
    ) -> Option<i32> {
        if card_count == 0 {
            return None;
        }
        let hovered = ui.rect_contains_pointer(rect);
        let now = ui.input(|input| input.time);
        let gesture_blocked = self.pointer_gesture_active();
        let mut direction = None;
        let mut consumed_wheel = false;
        ui.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } if hovered && !gesture_blocked && !modifiers.ctrl && !modifiers.command => {
                    consumed_wheel = true;
                    if *phase == egui::TouchPhase::Move {
                        self.carousel_wheel_accum += wheel_units(*unit, delta.y);
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
            if hovered && (consumed_wheel || now < self.carousel_wheel_suppress_until) {
                input.smooth_scroll_delta = Vec2::ZERO;
            }
        });

        if !hovered {
            // A fractional notch belongs to the region where it started. Do not let it
            // complete later after the pointer has moved to the rail or game window.
            self.carousel_wheel_accum = 0.0;
        }
        if consumed_wheel {
            self.carousel_wheel_last_event_at = now;
            self.carousel_wheel_suppress_until = now + WHEEL_IDLE_RESET_SECS;
            suppress_tooltips(ui.ctx());
        }
        if now - self.carousel_wheel_last_event_at > WHEEL_IDLE_RESET_SECS {
            self.carousel_wheel_accum = 0.0;
        }
        if hovered && self.carousel_wheel_accum != 0.0 {
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
            } else if consumed_wheel {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        WHEEL_STEP_COOLDOWN_SECS,
                    ));
            }
        }
        direction
    }

    fn consume_category_wheel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        visible_indices: &[usize],
    ) -> Option<usize> {
        if visible_indices.is_empty() {
            return None;
        }
        if !ui.rect_contains_pointer(rect) {
            // A partial wheel notch must never leak into the category rail after leaving it.
            self.category_wheel_accum = 0.0;
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
            suppress_tooltips(ui.ctx());
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

    fn apply_focused_costume_action(&mut self, action: ModAction, now: f64) {
        let category_index = self.selected_category;
        let costume_index = self.carousel_focus;
        if self.read_only || !self.visible_mods().contains(&costume_index) {
            return;
        }
        let mut changed = false;
        if let Some(category) = self.catalog.categories.get_mut(category_index) {
            if costume_index >= category.costumes.len() {
                return;
            }
            match action {
                ModAction::Exclusive => {
                    for (index, costume) in category.costumes.iter_mut().enumerate() {
                        let active = index == costume_index;
                        changed |= costume.active != active;
                        costume.active = active;
                    }
                }
                // Enabling through the toggle keeps the others enabled.
                ModAction::Toggle => {
                    let costume = &mut category.costumes[costume_index];
                    costume.active = !costume.active;
                    changed = true;
                }
            }
            self.active_images[category_index] = active_image(category);
        }
        if changed {
            self.active_feedback_until = now + ACTIVE_FEEDBACK_SECS;
            self.active_feedback_costume = Some(costume_index);
        }
    }

    fn texture_for(&mut self, ui: &Ui, path: Option<&Path>) -> Option<egui::TextureHandle> {
        self.thumbnails.get(ui.ctx(), path?)
    }
}

/// Queue the images of the mods the search shows around `focus`.
fn append_nearby_mod_images(
    paths: &mut Vec<PathBuf>,
    category: &Category,
    visible: &[usize],
    focus: usize,
    radius: usize,
) {
    let count = visible.len();
    if count == 0 {
        return;
    }
    let focus = visible
        .iter()
        .position(|&index| index == focus)
        .unwrap_or(0);
    paths.extend(category.costumes[visible[focus]].image.clone());
    for distance in 1..=radius.min(count - 1) {
        for position in [
            (focus + distance) % count,
            (focus + count - distance) % count,
        ] {
            if let Some(path) = &category.costumes[visible[position]].image
                && !paths.contains(path)
            {
                paths.push(path.clone());
            }
        }
    }
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

/// The mod `direction` steps to among the mods a search shows.  Stops at
/// either end, like the carousel without a search.
fn next_visible_mod(visible: &[usize], focus: usize, direction: i32) -> Option<usize> {
    let first = *visible.first()?;
    let Some(position) = visible.iter().position(|&index| index == focus) else {
        return Some(first);
    };
    Some(visible[next_focus_index(visible.len(), position, direction)])
}

fn next_focus_index(len: usize, current: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let current = current.min(len - 1);
    match direction.signum() {
        -1 if current == 0 => 0,
        -1 => current - 1,
        1 => (current + 1).min(len - 1),
        _ => current,
    }
}

fn card_reveal_progress(
    placement: &CarouselCardPlacement,
    progress: f32,
    card_count: usize,
) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    // Reveal the focused card first, then bring in its neighbors with a small stagger.
    // Two-card carousels have no center slot, so their first card still appears promptly.
    let delay = if placement.focused {
        0.0
    } else if card_count <= 2 {
        0.10
    } else if placement.slot == 0 {
        0.22
    } else {
        0.40
    };
    let local = ((progress - delay) / (1.0 - delay)).clamp(0.0, 1.0);
    local * local * (3.0 - 2.0 * local)
}

fn carousel_card_placements(
    carousel_rect: Rect,
    costume_count: usize,
    focus: usize,
) -> Vec<CarouselCardPlacement> {
    if costume_count == 0 {
        return Vec::new();
    }
    let focus = focus.min(costume_count - 1);
    let center_y = carousel_rect.center().y;
    let center_x = carousel_rect.center().x;
    let focused_center_x = if costume_count == 2 {
        // Keep a two-card group centered as a whole, regardless of which card is focused.
        let offset = (CAROUSEL_CARD_GAP + NEIGHBOR_CARD_SIZE.x) * 0.5;
        if focus == 0 {
            center_x - offset
        } else {
            center_x + offset
        }
    } else {
        center_x
    };
    let focused_rect =
        Rect::from_center_size(egui::pos2(focused_center_x, center_y), FOCUSED_CARD_SIZE);
    let mut placements = Vec::with_capacity(costume_count.min(3));
    if costume_count > 2 && focus > 0 {
        let left = focus - 1;
        placements.push(CarouselCardPlacement {
            index: left,
            rect: Rect::from_center_size(
                egui::pos2(
                    focused_rect.min.x - CAROUSEL_CARD_GAP - NEIGHBOR_CARD_SIZE.x * 0.5,
                    center_y,
                ),
                NEIGHBOR_CARD_SIZE,
            ),
            focused: false,
            slot: 0,
        });
    } else if costume_count == 2 && focus == 1 {
        placements.push(CarouselCardPlacement {
            index: 0,
            rect: Rect::from_center_size(
                egui::pos2(
                    focused_rect.min.x - CAROUSEL_CARD_GAP - NEIGHBOR_CARD_SIZE.x * 0.5,
                    center_y,
                ),
                NEIGHBOR_CARD_SIZE,
            ),
            focused: false,
            slot: 0,
        });
    }
    if costume_count > 2 && focus + 1 < costume_count {
        let right = focus + 1;
        placements.push(CarouselCardPlacement {
            index: right,
            rect: Rect::from_center_size(
                egui::pos2(
                    focused_rect.max.x + CAROUSEL_CARD_GAP + NEIGHBOR_CARD_SIZE.x * 0.5,
                    center_y,
                ),
                NEIGHBOR_CARD_SIZE,
            ),
            focused: false,
            slot: 2,
        });
    } else if costume_count == 2 && focus == 0 {
        placements.push(CarouselCardPlacement {
            index: 1,
            rect: Rect::from_center_size(
                egui::pos2(
                    focused_rect.max.x + CAROUSEL_CARD_GAP + NEIGHBOR_CARD_SIZE.x * 0.5,
                    center_y,
                ),
                NEIGHBOR_CARD_SIZE,
            ),
            focused: false,
            slot: 2,
        });
    }
    placements.push(CarouselCardPlacement {
        index: focus,
        rect: focused_rect,
        focused: true,
        slot: if costume_count == 2 {
            if focus == 0 { 0 } else { 2 }
        } else {
            1
        },
    });
    placements
}

/// Card placements for the mods a search shows.  They sit side by side as if
/// the hidden mods were not installed.
fn visible_card_placements(
    carousel_rect: Rect,
    visible: &[usize],
    focus: usize,
) -> Vec<CarouselCardPlacement> {
    let position = visible
        .iter()
        .position(|&index| index == focus)
        .unwrap_or(0);
    let mut placements = carousel_card_placements(carousel_rect, visible.len(), position);
    for placement in &mut placements {
        placement.index = visible[placement.index];
    }
    placements
}

fn lerp_rect(from: Rect, to: Rect, t: f32) -> Rect {
    Rect::from_min_max(lerp_pos(from.min, to.min, t), lerp_pos(from.max, to.max, t))
}

fn lerp_pos(from: egui::Pos2, to: egui::Pos2, t: f32) -> egui::Pos2 {
    from + (to - from) * t
}

fn same_card_geometry(from: &[CarouselCardPlacement], to: &[CarouselCardPlacement]) -> bool {
    from.len() == to.len()
        && to.iter().all(|target| {
            from.iter().any(|source| {
                source.index == target.index
                    && source.rect.min.distance(target.rect.min) < 0.5
                    && source.rect.max.distance(target.rect.max) < 0.5
                    && source.focused == target.focused
            })
        })
}

fn valid_carousel_release(
    press: &CarouselPointerPress,
    category_index: usize,
    pointer_pos: Option<egui::Pos2>,
    can_click: bool,
) -> Option<usize> {
    (press.category_index == category_index)
        .then_some(press)
        .filter(|press| can_click && pointer_pos.is_some_and(|pos| press.rect.contains(pos)))
        .map(|press| press.costume_index)
}

fn active_costume_index(category: &Category) -> usize {
    category
        .costumes
        .iter()
        .position(|costume| costume.active)
        .unwrap_or(0)
}

/// The pictures of the mods the library censors.
fn censored_images(catalog: &Catalog) -> HashSet<PathBuf> {
    catalog
        .categories
        .iter()
        .flat_map(|category| &category.costumes)
        .filter(|costume| costume.censored)
        .filter_map(|costume| costume.image.clone())
        .collect()
}

/// The rail's picture for a category without its own: the active mod's, else
/// the first mod's that has one.
fn active_image(category: &Category) -> Option<PathBuf> {
    category
        .costumes
        .iter()
        .find(|costume| costume.active)
        .and_then(|costume| costume.image.clone())
        .or_else(|| category.image.clone())
        .or_else(|| {
            category
                .costumes
                .iter()
                .find_map(|costume| costume.image.clone())
        })
}

fn character_name(name: &str) -> &str {
    name.strip_prefix("Operators: ").unwrap_or(name)
}

fn paint_category_name(ui: &Ui, rect: Rect, name: &str, font: FontId, color: Color32) -> bool {
    let mut job = egui::text::LayoutJob::simple(name.to_owned(), font, color, rect.width());
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    let position = egui::pos2(
        rect.center().x - galley.size().x * 0.5,
        rect.center().y - galley.size().y * 0.5,
    );
    let elided = galley.elided;
    ui.painter().galley(position, galley, color);
    elided
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

fn scaled_alpha(alpha: u8, factor: f32) -> u8 {
    (f32::from(alpha) * factor.clamp(0.0, 1.0)).round() as u8
}

fn scale_color_alpha(color: Color32, factor: f32) -> Color32 {
    // Color32 already stores premultiplied channels. Scale them together so
    // a completed reveal is exactly the original color, without a second tint.
    color.gamma_multiply(factor.clamp(0.0, 1.0))
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
    let image_rect = fitted_image_rect(rect, source_size);
    egui::Image::from_texture(texture)
        .tint(Color32::from_white_alpha(tint_alpha))
        .corner_radius(CornerRadius::ZERO)
        .paint_at(ui, image_rect);
}

fn fitted_image_rect(rect: Rect, source_size: Vec2) -> Rect {
    if source_size.x <= 0.0 || source_size.y <= 0.0 {
        return rect;
    }
    let scale = (rect.width() / source_size.x).min(rect.height() / source_size.y);
    Rect::from_center_size(rect.center(), source_size * scale)
}

fn paint_bottom_gradient(
    ui: &mut Ui,
    rect: Rect,
    overlay_opacity: u8,
    requested_height: f32,
    reveal: f32,
) {
    let gradient_height = (requested_height + 22.0).clamp(24.0, rect.height() * 0.56);
    // Fade above the title block, keeping the title itself on a dark base.
    let fade_end = ((gradient_height - requested_height) / gradient_height).max(0.01);
    let stops: [(f32, u8); 4] = [(0.0, 0), (fade_end * 0.4, 20), (fade_end, 150), (1.0, 218)];
    let start_y = rect.max.y - gradient_height;
    let mut mesh = egui::Mesh::default();
    for row in 0..=64 {
        let stop = row as f32 / 64.0;
        let interval = stops.windows(2).find(|pair| stop <= pair[1].0).unwrap();
        let blend = (stop - interval[0].0) / (interval[1].0 - interval[0].0);
        let alpha = (f32::from(interval[0].1)
            + blend * (f32::from(interval[1].1) - f32::from(interval[0].1)))
            as u8;
        let y = start_y + gradient_height * stop;
        let base = mesh.vertices.len() as u32;
        let color = scale_color_alpha(content_black_alpha(alpha, overlay_opacity), reveal);
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

fn card_name_rect(image_rect: Rect, focused: bool) -> Rect {
    let max_text_height = if focused { 34.0 } else { 29.0 };
    Rect::from_min_max(
        egui::pos2(
            image_rect.min.x + 8.0,
            image_rect.max.y - CARD_TITLE_BOTTOM_PADDING - max_text_height,
        ),
        egui::pos2(
            image_rect.max.x - 8.0,
            image_rect.max.y - CARD_TITLE_BOTTOM_PADDING,
        ),
    )
}

fn measure_card_name(
    ui: &Ui,
    rect: Rect,
    name: &str,
    overlay_opacity: u8,
    focused: bool,
) -> (Vec2, bool) {
    let galley = ui
        .painter()
        .layout_job(card_name_job(rect, name, overlay_opacity, focused));
    (galley.size(), galley.elided)
}

fn card_name_job(
    rect: Rect,
    name: &str,
    overlay_opacity: u8,
    focused: bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple(
        clean_display_name(name),
        FontId::proportional(if focused { 12.5 } else { 10.5 }),
        content_gray(255, overlay_opacity),
        rect.width(),
    );
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    job
}

fn paint_card_name_revealed(
    ui: &Ui,
    rect: Rect,
    name: &str,
    overlay_opacity: u8,
    focused: bool,
    reveal: f32,
) -> bool {
    let galley = ui
        .painter()
        .layout_job(card_name_job(rect, name, overlay_opacity, focused));
    let elided = galley.elided;
    let position = egui::pos2(rect.min.x, rect.max.y - galley.size().y);
    ui.painter().galley(
        position,
        galley,
        scale_color_alpha(content_gray(255, overlay_opacity), reveal),
    );
    elided
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
        category.costumes[index].active = !category.costumes[index].active;
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
    fn carousel_focus_stops_at_boundaries_without_wrapping() {
        assert_eq!(next_focus_index(3, 0, -1), 0);
        assert_eq!(next_focus_index(3, 2, 1), 2);
        assert_eq!(next_focus_index(1, 0, -1), 0);
        assert_eq!(next_focus_index(2, 0, 1), 1);
        assert_eq!(next_focus_index(2, 1, -1), 0);
    }

    #[test]
    fn endpoint_carousel_placements_show_only_real_neighbors() {
        let carousel = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(560.0, 266.0));
        let first = carousel_card_placements(carousel, 4, 0);
        let last = carousel_card_placements(carousel, 4, 3);
        assert_eq!(
            first.iter().map(|card| card.index).collect::<Vec<_>>(),
            vec![1, 0]
        );
        assert_eq!(
            last.iter().map(|card| card.index).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(first.iter().all(|card| card.index != 3));
        assert!(last.iter().all(|card| card.index != 0));
    }

    #[test]
    fn staged_reveal_makes_focused_card_available_before_neighbors() {
        let carousel = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(560.0, 266.0));
        let cards = carousel_card_placements(carousel, 3, 1);
        let focused = cards.iter().find(|card| card.focused).unwrap();
        let left = cards.iter().find(|card| card.slot == 0).unwrap();
        let right = cards.iter().find(|card| card.slot == 2).unwrap();
        assert!(card_reveal_progress(focused, 0.15, 3) > 0.0);
        assert_eq!(card_reveal_progress(left, 0.15, 3), 0.0);
        assert_eq!(card_reveal_progress(right, 0.30, 3), 0.0);
        assert!(card_reveal_progress(left, 0.30, 3) > 0.0);
        assert!(card_reveal_progress(right, 0.50, 3) > 0.0);
    }

    #[test]
    fn two_card_carousel_stays_balanced_when_focus_changes() {
        let carousel = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(560.0, 266.0));
        for focus in [0, 1] {
            let cards = carousel_card_placements(carousel, 2, focus);
            let min_x = cards
                .iter()
                .map(|card| card.rect.min.x)
                .fold(f32::INFINITY, f32::min);
            let max_x = cards
                .iter()
                .map(|card| card.rect.max.x)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(((min_x + max_x) * 0.5 - carousel.center().x).abs() < 0.01);
        }
    }

    #[test]
    fn focused_and_neighbor_cards_keep_the_same_aspect_ratio() {
        let focused_ratio = FOCUSED_CARD_SIZE.x / FOCUSED_CARD_SIZE.y;
        let neighbor_ratio = NEIGHBOR_CARD_SIZE.x / NEIGHBOR_CARD_SIZE.y;
        assert!((focused_ratio - neighbor_ratio).abs() < 0.01);
    }

    #[test]
    fn rapid_carousel_retarget_keeps_current_positions() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false, false])],
            note: None,
        });
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        let start = layouts.current_carousel_placements(rect, 1.0);
        layouts.carousel_focus = 1;
        layouts.begin_carousel_transition(start, rect, 1.0);
        let midway = layouts.current_carousel_placements(rect, 1.07);
        layouts.carousel_focus = 2;
        layouts.begin_carousel_transition(midway.clone(), rect, 1.07);
        let retargeted = layouts.current_carousel_placements(rect, 1.07);
        for before in &midway {
            if let Some(after) = retargeted.iter().find(|card| card.index == before.index) {
                assert_eq!(before.rect, after.rect);
            }
        }
        let settled = layouts.current_carousel_placements(rect, 2.0);
        assert!(same_card_geometry(
            &settled,
            &carousel_card_placements(rect, 3, 2)
        ));
        assert!(layouts.carousel_transition.is_none());
    }

    #[test]
    fn carousel_widget_ids_follow_visual_slots_across_category_changes() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false]), category(&[true, false])],
            note: None,
        });
        let context = egui::Context::default();
        let size = Vec2::new(560.0, 266.0);

        // Different categories reuse the same two-card layout.
        run_layout_frame(&context, &mut layouts, size, 1.0, Vec::new(), false);
        let left_before = context
            .read_response(carousel_slot_id(0))
            .expect("left carousel slot should be registered")
            .rect;
        let right_before = context
            .read_response(carousel_slot_id(2))
            .expect("right carousel slot should be registered")
            .rect;

        layouts.select_category(1);
        run_layout_frame(&context, &mut layouts, size, 1.1, Vec::new(), false);

        // If IDs were based on category/costume indices, these responses would
        // disappear when a different category replaced the cards in-place. This
        // is the same stable-rect condition egui uses for its warning.
        let left_after = context
            .read_response(carousel_slot_id(0))
            .expect("left carousel slot id must survive category changes")
            .rect;
        let right_after = context
            .read_response(carousel_slot_id(2))
            .expect("right carousel slot id must survive category changes")
            .rect;
        assert_eq!(left_before, left_after);
        assert_eq!(right_before, right_after);
    }

    #[test]
    fn carousel_slots_stay_unique_during_wrapping_animations() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        for count in 1..=5 {
            let mut layouts = Layouts::new(Catalog {
                game: "Test".into(),
                categories: vec![category(&vec![false; count])],
                note: None,
            });
            for focus in 0..count {
                let from = carousel_card_placements(rect, count, focus);
                layouts.carousel_focus = (focus + 1) % count;
                layouts.begin_carousel_transition(from, rect, 1.0);
                for step in 0..=14 {
                    let cards = layouts.current_carousel_placements(rect, 1.0 + step as f64 * 0.01);
                    let slots: std::collections::HashSet<_> =
                        cards.iter().map(|card| card.slot).collect();
                    assert_eq!(
                        slots.len(),
                        cards.len(),
                        "duplicate ids with {count} cards at step {step}"
                    );
                }
            }
        }
    }

    #[test]
    fn direct_card_activation_keeps_focus_and_transition_in_place() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false, false])],
            note: None,
        });
        layouts.carousel_focus = 0;
        layouts.carousel_transition = Some(CarouselTransition {
            from: carousel_card_placements(
                Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
                3,
                0,
            ),
            started_at: 1.0,
        });

        layouts.activate_costume_in_place(0, 2, 1.05);

        assert_eq!(layouts.carousel_focus, 0);
        assert!(layouts.carousel_transition.is_some());
        assert_eq!(layouts.catalog.categories[0].costumes[2].active, true);
        assert_eq!(layouts.catalog.categories[0].costumes[0].active, false);
        assert_eq!(layouts.active_feedback_costume, Some(2));
    }

    #[test]
    fn release_uses_the_pressed_card_even_if_current_geometry_moved() {
        let press = CarouselPointerPress {
            category_index: 0,
            costume_index: 2,
            rect: Rect::from_min_size(egui::pos2(20.0, 30.0), Vec2::new(80.0, 90.0)),
            placements: Vec::new(),
        };
        assert_eq!(
            valid_carousel_release(&press, 0, Some(egui::pos2(35.0, 45.0)), true),
            Some(2)
        );
        assert_eq!(
            valid_carousel_release(&press, 0, Some(egui::pos2(200.0, 200.0)), true),
            None
        );
        assert_eq!(
            valid_carousel_release(&press, 0, Some(egui::pos2(35.0, 45.0)), false),
            None
        );
        assert_eq!(
            valid_carousel_release(&press, 1, Some(egui::pos2(35.0, 45.0)), true),
            None
        );
    }

    #[test]
    fn frame_click_activates_side_card_without_recentering() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let side = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
        )
        .into_iter()
        .find(|card| card.index == 1)
        .expect("right side card");

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            vec![
                pointer_button_event(side.rect.center(), true),
                pointer_button_event(side.rect.center(), false),
            ],
            false,
        );

        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn frame_release_resumes_an_animation_from_the_frozen_press_geometry() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let carousel = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        let start = carousel_card_placements(carousel, 3, 0);
        layouts.carousel_focus = 1;
        layouts.begin_carousel_transition(start, carousel, 1.0);
        let midway = layouts.current_carousel_placements(carousel, 1.12);
        let side = midway
            .iter()
            .find(|card| card.index == 2)
            .expect("visible side card");
        // At this point the side card has cleared the focused card's hit rectangle. This
        // keeps the fixture about frozen geometry rather than the topmost-overlap rule.
        let side_pos = side.rect.right_center() - Vec2::new(4.0, 0.0);

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.12,
            vec![pointer_button_event(side_pos, true)],
            false,
        );
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.25,
            vec![pointer_button_event(side_pos, false)],
            false,
        );

        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![2]);
        let transition = layouts
            .carousel_transition
            .as_ref()
            .expect("the prior transition should continue");
        assert_eq!(
            transition
                .from
                .iter()
                .find(|card| card.index == 2)
                .unwrap()
                .rect,
            side.rect
        );
        assert_eq!(transition.started_at, 1.25);
    }

    #[test]
    fn frame_drag_cancels_a_pending_card_activation() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let side = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
        )
        .into_iter()
        .find(|card| card.index == 1)
        .expect("right side card");

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            vec![pointer_button_event(side.rect.center(), true)],
            false,
        );
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.05,
            vec![
                egui::Event::PointerMoved(egui::pos2(40.0, 40.0)),
                pointer_button_event(egui::pos2(40.0, 40.0), false),
            ],
            false,
        );

        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
    }

    #[test]
    fn frame_secondary_hold_does_not_activate_or_follow_the_pointer_target() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let side = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
        )
        .into_iter()
        .find(|card| card.index == 1)
        .expect("right side card");

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            vec![pointer_button_event_with_button(
                side.rect.center(),
                egui::PointerButton::Secondary,
                true,
            )],
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
        assert_eq!(
            layouts.held_preview.map(|preview| preview.costume_index),
            Some(1)
        );

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.1,
            vec![pointer_button_event_with_button(
                egui::pos2(540.0, 260.0),
                egui::PointerButton::Secondary,
                false,
            )],
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
        assert!(layouts.held_preview.is_none());
    }

    #[test]
    fn frame_wheel_routes_to_the_region_under_the_pointer() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.0,
            wheel_events(egui::pos2(280.0, 133.0), -1.0),
            true,
        );
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(layouts.selected_category, 0);

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.2,
            wheel_events(egui::pos2(280.0, 300.0), -1.0),
            true,
        );
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(
            layouts.carousel_focus,
            active_costume_index(&layouts.catalog.categories[1])
        );
    }

    #[test]
    fn fractional_wheel_input_does_not_leak_after_leaving_its_region() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.0,
            wheel_events_with_unit(egui::pos2(280.0, 133.0), -25.0, egui::MouseWheelUnit::Point),
            true,
        );
        assert_eq!(layouts.carousel_focus, 0);

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.2,
            vec![egui::Event::PointerMoved(egui::pos2(280.0, 300.0))],
            true,
        );
        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(layouts.selected_category, 0);
    }

    fn test_layouts(category_count: usize) -> Layouts {
        Layouts::new(Catalog {
            game: "Test".into(),
            categories: (0..category_count)
                .map(|index| Category {
                    id: format!("category-{index}"),
                    ..category(&[index == 0, false, false])
                })
                .collect(),
            note: None,
        })
    }

    fn run_layout_frame(
        context: &egui::Context,
        layouts: &mut Layouts,
        size: Vec2,
        time: f64,
        events: Vec<egui::Event>,
        show_category_strip: bool,
    ) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(egui::Pos2::ZERO, size));
        input.time = Some(time);
        input.events = events;
        super::super::apply_preview_style(context);
        let _ = context.run_ui(input, |ui| {
            layouts.show_carousel(ui, 94);
            if show_category_strip {
                layouts.show_category_strip(ui, 94);
            }
        });
    }

    fn pointer_button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        pointer_button_event_with_button(pos, egui::PointerButton::Primary, pressed)
    }

    fn pointer_button_event_with_button(
        pos: egui::Pos2,
        button: egui::PointerButton,
        pressed: bool,
    ) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn carousel_slot_id(slot: usize) -> egui::Id {
        egui::Id::new((egui::ViewportId::ROOT, "__top_ui"))
            .with(("overlay-preview-carousel-card-slot", slot))
    }

    fn wheel_events(pos: egui::Pos2, delta_y: f32) -> Vec<egui::Event> {
        wheel_events_with_unit(pos, delta_y, egui::MouseWheelUnit::Line)
    }

    fn wheel_events_with_unit(
        pos: egui::Pos2,
        delta_y: f32,
        unit: egui::MouseWheelUnit,
    ) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            // Keep the constructor in one place so the routing test exercises the same event
            // shape produced by winit, including a touch phase.
            egui::Event::MouseWheel {
                unit,
                delta: Vec2::new(0.0, delta_y),
                modifiers: egui::Modifiers::default(),
                phase: egui::TouchPhase::Move,
            },
        ]
    }

    fn active_indices(category: &Category) -> Vec<usize> {
        category
            .costumes
            .iter()
            .enumerate()
            .filter_map(|(index, costume)| costume.active.then_some(index))
            .collect()
    }

    fn category(active: &[bool]) -> Category {
        Category {
            id: "Ardelia".into(),
            name: "Ardelia".into(),
            image: None,
            costumes: active
                .iter()
                .enumerate()
                .map(|(index, &active)| Costume {
                    id: format!("costume-{index}"),
                    name: format!("Costume {index}"),
                    image: None,
                    active,
                    censored: false,
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

    #[test]
    fn selecting_an_active_costume_with_multiple_active_costumes_toggles_only_it() {
        let mut character = category(&[true, true, false]);
        select_costume(&mut character, 1);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![true, false, false]
        );
    }

    #[test]
    fn switching_categories_restores_each_category_focus() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![
                category(&[true, false, false]),
                category(&[true, false, false]),
            ],
            note: None,
        });
        layouts.carousel_focus = 2;
        layouts.select_category(1);
        assert_eq!(layouts.carousel_focus, 0);
        layouts.carousel_focus = 1;
        layouts.select_category(0);
        assert_eq!(layouts.carousel_focus, 2);
        layouts.select_category(1);
        assert_eq!(layouts.carousel_focus, 1);
    }

    #[test]
    fn queued_mod_navigation_moves_focus_without_activation() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        layouts.navigate_mod(&context, 1);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
    }

    #[test]
    fn direct_focused_exclusive_action_uses_the_same_selection_rule() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn exclusive_action_enables_focused_mod_and_disables_the_rest() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 2;
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![2]);
    }

    #[test]
    fn toggle_action_can_remove_the_last_active_mod() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[false, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 1;
        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert!(active_indices(&layouts.catalog.categories[0]).is_empty());
    }

    #[test]
    fn exclusive_action_is_a_noop_when_only_the_focused_mod_is_enabled() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[false, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 1;
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn toggle_action_enables_an_inactive_mod_without_disabling_others() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[false, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 0;
        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0, 1]);

        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.1,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn queued_navigation_and_action_are_applied_in_fifo_order() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();
        layouts.navigate_category(&context, 1);
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(active_indices(&layouts.catalog.categories[1]), vec![1]);
    }

    #[test]
    fn shortcut_availability_reflects_focused_mod_state() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false, false]), category(&[false, false])],
            note: None,
        });
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: true,
                mods: true,
                exclusive: false,
                toggle: true,
            }
        );

        layouts.carousel_focus = 1;
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: true,
                mods: true,
                exclusive: true,
                toggle: true,
            }
        );
    }

    #[test]
    fn shortcut_availability_handles_empty_category() {
        let layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[])],
            note: None,
        });
        let availability = layouts.shortcut_availability();
        assert!(!availability.categories);
        assert!(!availability.mods);
        assert!(!availability.exclusive);
        assert!(!availability.toggle);
    }

    #[test]
    fn shortcut_availability_marks_multiple_enabled_focus_as_actionable() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, true, false])],
            note: None,
        });
        layouts.carousel_focus = 1;
        let availability = layouts.shortcut_availability();
        assert!(availability.mods);
        assert!(availability.exclusive);
        assert!(availability.toggle);
    }

    #[test]
    fn queued_category_mod_exclusive_action_applies_in_order() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();
        layouts.navigate_category(&context, 1);
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(active_indices(&layouts.catalog.categories[1]), vec![1]);
    }

    #[test]
    fn rows_switch_up_and_down_and_reset_to_categories() {
        let mut layouts = test_layouts(2);
        assert_eq!(layouts.row(), Row::Categories);
        assert_eq!(layouts.selected_category, 0);
        layouts.switch_row(1);
        assert_eq!(layouts.row(), Row::Categories);
        layouts.switch_row(-1);
        assert_eq!(layouts.row(), Row::Mods);
        layouts.switch_row(-1);
        layouts.switch_row(0);
        assert_eq!(layouts.row(), Row::Mods);
        layouts.switch_row(1);
        assert_eq!(layouts.row(), Row::Categories);
        layouts.switch_row(-1);
        layouts.reset_row();
        assert_eq!(layouts.row(), Row::Categories);
    }

    #[test]
    fn mod_and_category_steps_and_actions_work_from_either_row() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();
        layouts.switch_row(1);
        layouts.navigate_mod(&context, 1);
        layouts.navigate_category(&context, 1);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.row(), Row::Categories);
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.carousel_focus_by_category[0], 1);

        // Enabling acts on the focused mod while the categories row is current.
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.1,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.row(), Row::Categories);
        assert_eq!(active_indices(&layouts.catalog.categories[1]), vec![1]);

        layouts.switch_row(-1);
        layouts.navigate_category(&context, -1);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.2,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.row(), Row::Mods);
        assert_eq!(layouts.selected_category, 0);
        assert_eq!(layouts.carousel_focus, 1);
    }

    fn named(name: &str, mods: &[&str]) -> Category {
        Category {
            id: name.into(),
            name: name.into(),
            image: None,
            costumes: mods
                .iter()
                .map(|name| Costume {
                    id: (*name).into(),
                    name: (*name).into(),
                    image: None,
                    active: false,
                    censored: false,
                })
                .collect(),
        }
    }

    fn search_layouts() -> Layouts {
        Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![
                named(
                    "Operators: Ardelia",
                    &["Summer Vow", "Beach_Day", "vow_of_dawn", "Classic"],
                ),
                named("Endministrator", &[]),
                named("Perlica", &["Night  Vow v1.2", "Classic"]),
                named("Vow Keepers", &["Alpha", "Beta"]),
            ],
            note: None,
        })
    }

    #[test]
    fn search_matches_names_loosely() {
        let layouts = search_layouts();
        let filter = |query: &str| Filter::new(&layouts.catalog, query);

        // Case, underscores and repeated spaces do not matter.
        let vow = filter("VOW");
        assert_eq!(vow.categories, vec![0, 2, 3]);
        assert_eq!(vow.mods, vec![vec![0, 2], vec![], vec![0], vec![0, 1]]);
        assert_eq!(filter("beach day").mods[0], vec![1]);
        assert_eq!(filter("vow of").mods[0], vec![2]);
        assert_eq!(filter("night vow v1.2").mods[2], vec![0]);

        // A matching category shows all its mods, even when it has none.
        let keepers = filter("keep");
        assert_eq!(keepers.categories, vec![3]);
        assert_eq!(keepers.mods[3], vec![0, 1]);
        assert_eq!(filter("endmin").categories, vec![1]);

        // A mod name can match in several categories.
        let classic = filter("classic");
        assert_eq!(classic.categories, vec![0, 2]);
        assert_eq!(classic.mods, vec![vec![3], vec![], vec![1], vec![]]);

        // The rail shows the character name, so the group prefix does not match.
        assert!(filter("operators").categories.is_empty());

        // An empty search, or one of only spaces, shows everything.
        for query in ["", "   "] {
            let all = filter(query);
            assert_eq!(all.categories, vec![0, 1, 2, 3]);
            assert_eq!(
                all.mods,
                vec![vec![0, 1, 2, 3], vec![], vec![0, 1], vec![0, 1]]
            );
        }
    }

    #[test]
    fn search_keeps_a_selection_it_shows_and_otherwise_moves_to_the_first_match() {
        let mut layouts = search_layouts();
        assert_eq!(layouts.selected_category, 0);
        layouts.carousel_focus = 2;

        // Ardelia still shows the focused mod.
        layouts.set_search("vow");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (0, 2));

        // Ardelia still shows, but not the focused mod.
        layouts.set_search("beach");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (0, 1));
        assert_eq!(layouts.carousel_focus_by_category[0], 1);

        // Ardelia is hidden, so the first match takes over, and Perlica's
        // remembered mod gives way to the one that matches.
        layouts.carousel_focus_by_category[2] = 1;
        layouts.set_search("night");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (2, 0));

        // Clearing the search stays where the search led.
        layouts.set_search("");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (2, 0));
    }

    #[test]
    fn keys_step_over_what_the_search_hides() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        let size = Vec2::new(560.0, 266.0);
        layouts.set_search("vow");

        layouts.navigate_mod(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.0, Vec::new(), false);
        assert_eq!(layouts.carousel_focus, 2);

        // The last match is an end, like the last mod.
        layouts.navigate_mod(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.1, Vec::new(), false);
        assert_eq!(layouts.carousel_focus, 2);
        layouts.navigate_mod(&context, -1);
        run_layout_frame(&context, &mut layouts, size, 1.2, Vec::new(), false);
        assert_eq!(layouts.carousel_focus, 0);

        // Endministrator is hidden.
        layouts.navigate_category(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.3, Vec::new(), false);
        assert_eq!(layouts.selected_category, 2);
        layouts.navigate_category(&context, 1);
        layouts.navigate_category(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.4, Vec::new(), false);
        assert_eq!(layouts.selected_category, 3);
        layouts.navigate_category(&context, -1);
        layouts.navigate_category(&context, -1);
        run_layout_frame(&context, &mut layouts, size, 1.5, Vec::new(), false);
        assert_eq!(layouts.selected_category, 0);
    }

    #[test]
    fn search_shows_only_matching_cards() {
        let mut layouts = search_layouts();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        layouts.set_search("vow");
        // Two matches lay out like a two-mod carousel.
        let cards = layouts.current_carousel_placements(rect, 1.0);
        let mut shown = cards.iter().map(|card| card.index).collect::<Vec<_>>();
        shown.sort_unstable();
        assert_eq!(shown, vec![0, 2]);
        let two = carousel_card_placements(rect, 2, 0);
        for (card, plain) in cards.iter().zip(&two) {
            assert_eq!((card.rect, card.slot), (plain.rect, plain.slot));
        }
    }

    #[test]
    fn enabling_a_search_result_still_disables_the_hidden_mods() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        layouts.catalog.categories[0].costumes[1].active = true;
        layouts.catalog.categories[0].costumes[3].active = true;
        layouts.set_search("dawn");
        assert_eq!(layouts.carousel_focus, 2);

        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![2]);
    }

    #[test]
    fn a_mod_the_search_hides_cannot_be_enabled() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        layouts.set_search("dawn");
        layouts.carousel_focus = 1;

        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert!(active_indices(&layouts.catalog.categories[0]).is_empty());
    }

    #[test]
    fn shortcut_availability_follows_the_search() {
        let mut layouts = search_layouts();
        // One category and one mod show, so there is nowhere to step.
        layouts.set_search("dawn");
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: false,
                mods: false,
                exclusive: true,
                toggle: true,
            }
        );

        layouts.set_search("vow");
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: true,
                mods: true,
                exclusive: true,
                toggle: true,
            }
        );

        layouts.carousel_focus = 1;
        let hidden = layouts.shortcut_availability();
        assert!(!hidden.exclusive && !hidden.toggle);

        layouts.set_search("zzz");
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: false,
                mods: false,
                exclusive: false,
                toggle: false,
            }
        );
    }

    #[test]
    fn a_search_without_matches_says_so() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        layouts.set_search("zzz");
        assert_eq!(layouts.empty_carousel_text(), "No matches");

        // The selection stays put, the rail is empty, and the carousel shows
        // only the notice.
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.0,
            Vec::new(),
            true,
        );
        assert_eq!(layouts.selected_category, 0);
        assert_eq!(layouts.visible_card_rects().len(), 1);

        // A category that matches by name but has no mods is still empty.
        layouts.set_search("endmin");
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.empty_carousel_text(), "No installed mods");
    }

    fn with_active(mut category: Category, active: &str) -> Category {
        for costume in &mut category.costumes {
            costume.active = costume.id == active;
        }
        category
    }

    fn live_catalog(categories: Vec<Category>) -> Catalog {
        Catalog {
            game: "Test".into(),
            categories,
            note: None,
        }
    }

    #[test]
    fn a_new_library_keeps_the_selection_by_id() {
        let mut layouts = Layouts::new(live_catalog(vec![
            named("Akekuri", &["A1", "A2"]),
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            named("Perlica", &["P1", "P2"]),
        ]));
        layouts.select_category(1);
        layouts.carousel_focus = 2;
        layouts.carousel_focus_by_category[2] = 1;
        assert_eq!(
            layouts.selection(),
            Selection {
                category_id: Some("Ardelia".into()),
                mods: [
                    ("Ardelia".to_owned(), "Classic".to_owned()),
                    ("Perlica".to_owned(), "P2".to_owned()),
                ]
                .into(),
            }
        );

        // A new category sorts first and Ardelia gains a mod before Classic.
        layouts.replace_catalog(live_catalog(vec![
            named("Aglina", &["G1"]),
            named("Akekuri", &["A1", "A2"]),
            named("Ardelia", &["Vow", "Beach", "Autumn", "Classic"]),
            named("Perlica", &["P1", "P2"]),
        ]));
        assert_eq!(layouts.selected_category, 2);
        assert_eq!(layouts.carousel_focus, 3);
        assert_eq!(layouts.carousel_focus_by_category[3], 1);
        assert!(layouts.reveal_category);

        // Without Ardelia the selection stays at its place in the rail.
        layouts.replace_catalog(live_catalog(vec![
            named("Aglina", &["G1"]),
            named("Akekuri", &["A1", "A2"]),
            named("Perlica", &["P1", "P2"]),
        ]));
        assert_eq!(layouts.selected_category, 2);
        assert_eq!(layouts.carousel_focus, 1);
    }

    #[test]
    fn a_category_left_on_its_active_mod_follows_the_active_mod() {
        let mut layouts = Layouts::new(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Vow",
        )]));
        assert_eq!(layouts.selection().mods.len(), 0);
        layouts.replace_catalog(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Classic",
        )]));
        assert_eq!(layouts.carousel_focus, 2);

        // Once moved away from the active mod, the focus stays put.
        layouts.carousel_focus = 1;
        layouts.replace_catalog(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Vow",
        )]));
        assert_eq!(layouts.carousel_focus, 1);
    }

    #[test]
    fn a_new_library_keeps_the_search() {
        let mut layouts = search_layouts();
        layouts.set_search("classic");
        layouts.select_category(2);
        layouts.carousel_focus = 1;
        let mut catalog = search_layouts().catalog;
        catalog.categories.remove(1);
        layouts.replace_catalog(catalog);
        assert_eq!(layouts.filter.query, "classic");
        assert_eq!(layouts.filter.categories, vec![0, 1]);
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (1, 1));
    }

    #[test]
    fn a_restored_selection_skips_what_is_gone() {
        let mut layouts = Layouts::new(live_catalog(vec![
            named("Akekuri", &["A1", "A2"]),
            named("Ardelia", &["Vow", "Beach"]),
        ]));
        layouts.restore_selection(&Selection {
            category_id: Some("Ardelia".into()),
            mods: [
                ("Ardelia".to_owned(), "Beach".to_owned()),
                ("Akekuri".to_owned(), "Removed".to_owned()),
            ]
            .into(),
        });
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (1, 1));
        assert_eq!(layouts.carousel_focus_by_category[0], 0);
        layouts.restore_selection(&Selection {
            category_id: Some("Removed".into()),
            mods: Default::default(),
        });
        assert_eq!(layouts.selected_category, 0);
    }

    #[test]
    fn read_only_keys_and_clicks_leave_mods_alone() {
        let mut layouts = Layouts::new(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach"]),
            "Vow",
        )]));
        layouts.set_read_only(true);
        layouts.carousel_focus = 1;
        let availability = layouts.shortcut_availability();
        assert!(!availability.exclusive && !availability.toggle);
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        layouts.apply_focused_costume_action(ModAction::Toggle, 1.0);
        layouts.activate_costume_in_place(0, 1, 1.0);
        let active: Vec<bool> = layouts.catalog.categories[0]
            .costumes
            .iter()
            .map(|costume| costume.active)
            .collect();
        assert_eq!(active, [true, false]);
    }
}
