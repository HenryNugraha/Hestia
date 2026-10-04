//! The hotkeys on the back of the focused mod card.  Hestia reads a mod's
//! hotkeys once its card is focused, and the card gets a keys button when it
//! has any.  The button, a click on the back, or R turns the card over.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{
    Align2, Color32, CornerRadius, FontId, Galley, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::{
    ACCENT, chrome_alpha, clean_display_name, content_color, content_gray, elided_galley,
    rgba_alpha, scale_color_alpha, text, wheel_units,
};
use crate::app::TextKey;
use crate::overlay_protocol::{FromOverlay, ModHotkey, ModHotkeys};

/// How long a card takes to turn over.
const FLIP_SECS: f64 = 0.24;
const BUTTON_SIZE: f32 = 21.0;
/// How long the keys button takes to slide its label out.
const LABEL_SECS: f32 = 0.12;
/// Before the keys button's label.
const LABEL_PADDING: f32 = 5.0;
/// Between the label and the icon's box, which has a margin of its own.
const LABEL_GAP: f32 = 2.0;
/// From the card's edges to its badges.
const INSET: f32 = 8.0;
const PADDING: f32 = 10.0;
/// The mod's name above the list.
const HEADER_HEIGHT: f32 = 37.0;
const ROW_HEIGHT: f32 = 24.0;
const KEYCAP_HEIGHT: f32 = 18.0;

/// Which side of a card shows, and how wide it is while it turns, 1 being
/// its full width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Face {
    pub back: bool,
    pub width: f32,
}

impl Face {
    pub(super) const FRONT: Self = Self {
        back: false,
        width: 1.0,
    };

    /// Not turning.
    pub(super) fn settled(self) -> bool {
        self.width >= 1.0
    }
}

/// The keys button's place this frame, worked out before what's under it
/// is painted.
pub(super) struct KeysButton {
    id: egui::Id,
    back: bool,
    rect: Rect,
    label: Arc<Galley>,
    /// How far the label has slid out, from 0 to 1.
    pub open: f32,
}

/// The keys button's label, out while the button is hovered.
#[derive(Default)]
struct Label {
    /// The button it was last on.
    id: Option<egui::Id>,
    /// How far it's out, from 0 to 1, before easing.
    open: f32,
    /// The pass it was last updated in.  A button that wasn't there the pass
    /// before starts with it in.
    pass: u64,
    /// A turn keeps it in until the pointer leaves the button, which is
    /// still under the pointer that clicked it.
    held: bool,
}

impl Label {
    /// How far it was out, eased, which is where the button still is.
    fn shown(&self, id: egui::Id, pass: u64) -> f32 {
        if self.id == Some(id) && self.pass + 1 >= pass {
            egui::emath::easing::cubic_out(self.open)
        } else {
            0.0
        }
    }

    /// Moves it out while the button is hovered, or back in, by `dt`
    /// seconds.  How far it's out, eased.
    fn update(&mut self, id: egui::Id, hovered: bool, pass: u64, dt: f32) -> f32 {
        if self.id != Some(id) || self.pass + 1 < pass {
            self.open = 0.0;
        }
        self.id = Some(id);
        self.pass = pass;
        self.held &= hovered;
        let step = dt / LABEL_SECS;
        self.open = if hovered && !self.held {
            (self.open + step).min(1.0)
        } else {
            (self.open - step).max(0.0)
        };
        egui::emath::easing::cubic_out(self.open)
    }

    fn moving(&self) -> bool {
        self.open > 0.0 && self.open < 1.0
    }

    /// Keeps it in until the pointer leaves the button.
    fn hold(&mut self) {
        self.held = true;
        self.open = 0.0;
    }
}

struct Flip {
    mod_id: String,
    /// The card shows, or turns to, its back.
    back: bool,
    started_at: f64,
    /// The first row the back shows.
    scroll: usize,
}

impl Flip {
    /// How far the card has turned, from its front at 0 to its back at 1.
    fn turn(&self, now: f64) -> f32 {
        let progress = ((now - self.started_at) / FLIP_SECS).clamp(0.0, 1.0) as f32;
        if self.back { progress } else { 1.0 - progress }
    }

    fn turning(&self, now: f64) -> bool {
        now - self.started_at < FLIP_SECS
    }
}

#[derive(Default)]
pub(super) struct CardKeys {
    /// Mods' hotkeys, by id, once Hestia has sent them.
    hotkeys: HashMap<String, Vec<ModHotkey>>,
    /// The library each mod was last asked about in.  A new library asks
    /// again, since the mod may have changed.
    asked: HashMap<String, u64>,
    library: u64,
    requests: Vec<FromOverlay>,
    flip: Option<Flip>,
    /// The button and the back painted this frame, and last frame.  The
    /// carousel leaves presses on them to their own clicks.
    own_clicks: Vec<Rect>,
    last_own_clicks: Vec<Rect>,
    /// The settled back painted this frame, and last frame, with how many
    /// of its rows didn't fit.
    back: Option<Rect>,
    last_back: Option<Rect>,
    hidden_rows: usize,
    wheel: f32,
    label: Label,
}

impl CardKeys {
    /// Asks Hestia for the mod's hotkeys, once per library.
    pub(super) fn ask(&mut self, mod_id: &str) {
        if self.asked.get(mod_id) != Some(&self.library) {
            self.asked.insert(mod_id.to_owned(), self.library);
            self.requests.push(FromOverlay::Hotkeys {
                mod_id: mod_id.to_owned(),
            });
        }
    }

    /// What to ask Hestia for since the last call.
    pub(super) fn take_requests(&mut self) -> Vec<FromOverlay> {
        std::mem::take(&mut self.requests)
    }

    pub(super) fn receive(&mut self, answer: ModHotkeys) {
        self.hotkeys.insert(answer.mod_id, answer.hotkeys);
    }

    /// Hestia sent a new library.  The hotkeys known so far still show
    /// until Hestia sends them again.
    pub(super) fn library_changed(&mut self) {
        self.library += 1;
    }

    pub(super) fn has(&self, mod_id: &str) -> bool {
        self.hotkeys
            .get(mod_id)
            .is_some_and(|list| !list.is_empty())
    }

    /// Turns the mod's card over, or back.  Nothing for a mod without
    /// hotkeys.
    pub(super) fn toggle(&mut self, mod_id: &str, now: f64) {
        let has = self.has(mod_id);
        match &mut self.flip {
            Some(flip) if flip.mod_id == mod_id => {
                // Midway, it turns back from where it is.
                let turn = flip.turn(now);
                flip.back = !flip.back;
                let remaining = if flip.back { 1.0 - turn } else { turn };
                flip.started_at = now - f64::from(1.0 - remaining) * FLIP_SECS;
            }
            _ if has => {
                self.flip = Some(Flip {
                    mod_id: mod_id.to_owned(),
                    back: true,
                    started_at: now,
                    scroll: 0,
                });
            }
            _ => return,
        }
        self.label.hold();
    }

    /// Shows the front of every card again.
    pub(super) fn reset(&mut self) {
        self.flip = None;
        self.back = None;
        self.last_back = None;
    }

    /// Only the focused card turns over.  One that lost the focus shows its
    /// front again.
    pub(super) fn keep_focused(&mut self, focused: Option<&str>) {
        if self
            .flip
            .as_ref()
            .is_some_and(|flip| Some(flip.mod_id.as_str()) != focused)
        {
            self.reset();
        }
    }

    pub(super) fn face(&mut self, mod_id: &str, now: f64) -> Face {
        let Some(flip) = self.flip.as_ref().filter(|flip| flip.mod_id == mod_id) else {
            return Face::FRONT;
        };
        let turn = flip.turn(now);
        if !flip.back && turn <= 0.0 {
            self.reset();
            return Face::FRONT;
        }
        // Each side narrows to nothing, or widens from it, for half the time.
        let angle = turn * std::f32::consts::PI;
        Face {
            back: turn >= 0.5,
            width: angle.cos().abs(),
        }
    }

    pub(super) fn turning(&self, now: f64) -> bool {
        self.flip.as_ref().is_some_and(|flip| flip.turning(now))
    }

    /// Call before anything reads `owns_press` this frame.
    pub(super) fn begin_frame(&mut self) {
        self.last_own_clicks = std::mem::take(&mut self.own_clicks);
        self.last_back = self.back.take();
    }

    /// Whether a press there belongs to the keys button or the back, not to
    /// the carousel.
    pub(super) fn owns_press(&self, pos: egui::Pos2) -> bool {
        self.last_own_clicks.iter().any(|rect| rect.contains(pos))
    }

    /// Scrolls a back whose rows don't fit with the wheel over it, which then
    /// doesn't move the carousel.
    pub(super) fn scroll_with_wheel(&mut self, ui: &mut Ui) {
        let (Some(flip), Some(back)) = (&mut self.flip, self.last_back) else {
            return;
        };
        if self.hidden_rows == 0 || !ui.rect_contains_pointer(back) {
            return;
        }
        let mut steps = 0.0;
        ui.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } if !modifiers.ctrl && !modifiers.command => {
                    if *phase == egui::TouchPhase::Move {
                        steps += wheel_units(*unit, delta.y);
                    }
                    false
                }
                _ => true,
            });
            input
                .raw
                .events
                .retain(|event| !matches!(event, egui::Event::MouseWheel { .. }));
            input.smooth_scroll_delta = Vec2::ZERO;
        });
        self.wheel += steps;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            flip.scroll = flip.scroll.saturating_sub(1);
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            flip.scroll = (flip.scroll + 1).min(self.hidden_rows);
        }
    }

    /// The keys button in the corner of `card`, a lit one on its back.
    /// Hovered, it slides out what it does to its left.
    pub(super) fn keys_button(
        &mut self,
        ui: &Ui,
        card: Rect,
        id: egui::Id,
        back: bool,
        overlay_opacity: u8,
    ) -> KeysButton {
        let label = elided_galley(
            ui,
            text(if back {
                TextKey::GameOverlayHideHotkeys
            } else {
                TextKey::GameOverlayShowHotkeys
            }),
            FontId::proportional(11.0),
            content_gray(235, overlay_opacity),
            // Clear of the tick box in the other corner.
            card.width() - 2.0 * INSET - BUTTON_ROOM - BUTTON_SIZE - LABEL_GAP - LABEL_PADDING,
        );
        let icon = button_rect(card);
        let open_width = BUTTON_SIZE + LABEL_GAP + label.size().x + LABEL_PADDING;
        let place = |open: f32| {
            let width = egui::lerp(BUTTON_SIZE..=open_width, open);
            Rect::from_min_max(egui::pos2(icon.max.x - width, icon.min.y), icon.max)
        };
        let pass = ui.ctx().cumulative_pass_nr();
        let hovered = ui.rect_contains_pointer(place(self.label.shown(id, pass)));
        let dt = ui.input(|input| input.stable_dt).min(0.1);
        let open = self.label.update(id, hovered, pass, dt);
        if self.label.moving() {
            ui.ctx().request_repaint();
        }
        KeysButton {
            id,
            back,
            rect: place(open),
            label,
            open,
        }
    }

    /// Paints the keys button.  True when it was clicked.
    pub(super) fn button(&mut self, ui: &Ui, button: KeysButton, overlay_opacity: u8) -> bool {
        let KeysButton {
            id,
            back,
            rect,
            label,
            open,
        } = button;
        self.own_clicks.push(rect);
        let response = ui
            .interact(rect, id, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let hovered = response.hovered();
        let fill = Color32::from_black_alpha(chrome_alpha(
            if hovered { 225 } else { 185 },
            overlay_opacity,
        ));
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
        let color = if back {
            content_color(ACCENT, overlay_opacity)
        } else {
            content_gray(if hovered { 255 } else { 225 }, overlay_opacity)
        };
        let icon = Rect::from_min_max(egui::pos2(rect.max.x - BUTTON_SIZE, rect.min.y), rect.max);
        ui.painter().text(
            icon.center(),
            Align2::CENTER_CENTER,
            char::from(lucide_icons::Icon::Keyboard).to_string(),
            FontId::new(13.0, egui::FontFamily::Name("preview-icons".into())),
            color,
        );
        if open > 0.0 {
            let pos = egui::pos2(
                icon.min.x - LABEL_GAP - label.size().x,
                rect.center().y - label.size().y * 0.5,
            );
            ui.painter()
                .with_clip_rect(rect)
                .galley_with_override_text_color(
                    pos,
                    label,
                    scale_color_alpha(content_gray(235, overlay_opacity), open),
                );
        }
        response.clicked()
    }

    /// The back of the mod's card: its name, then its hotkeys.  `card` is
    /// where the whole card goes, `shown` the part that shows while it
    /// turns.  True when its keys button was clicked.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_back(
        &mut self,
        ui: &Ui,
        card: Rect,
        shown: Rect,
        mod_id: &str,
        name: &str,
        border: Stroke,
        reveal: f32,
        overlay_opacity: u8,
        button: Option<KeysButton>,
    ) -> bool {
        let settled = shown.width() >= card.width();
        if settled {
            self.own_clicks.push(card);
            self.back = Some(card);
        }
        let painter = ui.painter().with_clip_rect(shown);
        let fade = |color: Color32| scale_color_alpha(color, reveal);
        painter.rect_filled(
            shown,
            CornerRadius::ZERO,
            fade(rgba_alpha(24, 24, 24, 242, overlay_opacity)),
        );
        let inner = card.shrink(PADDING);
        let title = elided_galley(
            ui,
            &clean_display_name(name),
            FontId::proportional(13.0),
            fade(content_gray(240, overlay_opacity)),
            // Up to the button, however far its label is out.
            button
                .as_ref()
                .map_or_else(|| button_rect(card), |button| button.rect)
                .min
                .x
                - 8.0
                - inner.min.x,
        );
        let title_y = card.min.y + INSET + (BUTTON_SIZE - title.size().y) * 0.5;
        painter.galley(
            egui::pos2(inner.min.x, title_y),
            title,
            Color32::PLACEHOLDER,
        );
        let divider_y = card.min.y + HEADER_HEIGHT;
        painter.line_segment(
            [
                egui::pos2(inner.min.x, divider_y),
                egui::pos2(inner.max.x, divider_y),
            ],
            Stroke::new(1.0, fade(content_gray(68, overlay_opacity))),
        );

        let rows = self
            .hotkeys
            .get(mod_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let list = Rect::from_min_max(egui::pos2(inner.min.x, divider_y + 6.0), inner.max);
        let fit = ((list.height() / ROW_HEIGHT).floor() as usize).max(1);
        let hidden = rows.len().saturating_sub(fit);
        if settled {
            self.hidden_rows = hidden;
        }
        let first = self.flip.as_mut().map_or(0, |flip| {
            flip.scroll = flip.scroll.min(hidden);
            flip.scroll
        });
        let key_font = FontId::proportional(11.0);
        let label_font = FontId::proportional(12.0);
        let key_color = fade(content_gray(235, overlay_opacity));
        // The keys line up in a column as wide as the widest, up to half
        // the card.
        let room = list.width() - if hidden > 0 { 6.0 } else { 0.0 };
        let column = rows
            .iter()
            .map(|row| {
                ui.painter()
                    .layout_no_wrap(row.key.clone(), key_font.clone(), key_color)
                    .size()
                    .x
                    + 10.0
            })
            .fold(0.0_f32, f32::max)
            .min(room * 0.5);
        for (line, row) in rows.iter().skip(first).take(fit).enumerate() {
            let y = list.min.y + line as f32 * ROW_HEIGHT + ROW_HEIGHT * 0.5;
            let key = elided_galley(ui, &row.key, key_font.clone(), key_color, column - 10.0);
            let keycap = Rect::from_min_size(
                egui::pos2(list.min.x, y - KEYCAP_HEIGHT * 0.5),
                Vec2::new(key.size().x + 10.0, KEYCAP_HEIGHT),
            );
            painter.rect_filled(
                keycap,
                CornerRadius::same(3),
                fade(rgba_alpha(58, 58, 58, 255, overlay_opacity)),
            );
            painter.rect_stroke(
                keycap,
                CornerRadius::same(3),
                Stroke::new(1.0, fade(content_gray(96, overlay_opacity))),
                StrokeKind::Inside,
            );
            painter.galley(
                keycap.center() - key.size() * 0.5,
                key,
                Color32::PLACEHOLDER,
            );
            let label = elided_galley(
                ui,
                &row.label,
                label_font.clone(),
                fade(content_gray(214, overlay_opacity)),
                room - column - 8.0,
            );
            painter.galley(
                egui::pos2(list.min.x + column + 8.0, y - label.size().y * 0.5),
                label,
                Color32::PLACEHOLDER,
            );
        }
        if hidden > 0 {
            // A thin bar on the right says where the rows are.
            let track = Rect::from_min_max(
                egui::pos2(list.max.x - 2.0, list.min.y),
                egui::pos2(list.max.x, list.min.y + fit as f32 * ROW_HEIGHT),
            );
            let share = fit as f32 / rows.len() as f32;
            let thumb_height = (track.height() * share).max(12.0);
            let offset = (track.height() - thumb_height) * first as f32 / hidden as f32;
            painter.rect_filled(
                track,
                CornerRadius::same(1),
                fade(content_gray(52, overlay_opacity)),
            );
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(track.min.x, track.min.y + offset),
                    Vec2::new(track.width(), thumb_height),
                ),
                CornerRadius::same(1),
                fade(content_gray(150, overlay_opacity)),
            );
        }
        painter.rect_stroke(
            shown.shrink(0.5),
            CornerRadius::ZERO,
            Stroke::new(border.width, fade(border.color)),
            StrokeKind::Inside,
        );
        button.is_some_and(|button| self.button(ui, button, overlay_opacity))
    }
}

/// Where the keys button goes on a card.
pub(super) fn button_rect(card: Rect) -> Rect {
    Rect::from_min_size(
        egui::pos2(card.max.x - INSET - BUTTON_SIZE, card.min.y + INSET),
        Vec2::splat(BUTTON_SIZE),
    )
}

/// The room the keys button takes from the NEW tag beside it.
pub(super) const BUTTON_ROOM: f32 = BUTTON_SIZE + 4.0;

/// A card narrowed around its middle while it turns.
pub(super) fn narrowed(card: Rect, width: f32) -> Rect {
    Rect::from_center_size(
        card.center(),
        Vec2::new(card.width() * width.clamp(0.0, 1.0), card.height()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(mod_id: &str, count: usize) -> ModHotkeys {
        ModHotkeys {
            mod_id: mod_id.into(),
            hotkeys: (0..count)
                .map(|index| ModHotkey {
                    key: format!("F{index}"),
                    label: format!("Key {index}"),
                })
                .collect(),
        }
    }

    #[test]
    fn a_turn_keeps_the_label_in_until_the_pointer_leaves() {
        let id = egui::Id::new("keys");
        let mut label = Label::default();
        assert_eq!(label.update(id, true, 1, LABEL_SECS), 1.0);
        label.hold();
        // The button comes back under the pointer once the card has turned.
        assert_eq!(label.update(id, true, 20, LABEL_SECS), 0.0);
        assert_eq!(label.update(id, true, 21, LABEL_SECS), 0.0);
        assert_eq!(label.update(id, false, 22, LABEL_SECS), 0.0);
        assert_eq!(label.update(id, true, 23, LABEL_SECS), 1.0);
    }

    #[test]
    fn a_button_that_was_away_starts_with_its_label_in() {
        let id = egui::Id::new("keys");
        let mut label = Label::default();
        label.update(id, true, 1, LABEL_SECS);
        assert_eq!(label.shown(id, 2), 1.0);
        assert_eq!(label.shown(id, 3), 0.0);
        assert_eq!(label.shown(egui::Id::new("other"), 2), 0.0);
        assert!(label.update(id, true, 5, LABEL_SECS * 0.5) < 1.0);
    }

    #[test]
    fn turning_holds_the_label_but_a_card_without_hotkeys_does_not() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 2));
        keys.toggle("none", 0.0);
        assert!(!keys.label.held);
        keys.toggle("some", 0.0);
        assert!(keys.label.held);
    }

    #[test]
    fn asks_once_per_library() {
        let mut keys = CardKeys::default();
        keys.ask("a");
        keys.ask("a");
        assert_eq!(
            keys.take_requests(),
            vec![FromOverlay::Hotkeys { mod_id: "a".into() }]
        );
        keys.ask("a");
        assert!(keys.take_requests().is_empty());
        keys.library_changed();
        keys.ask("a");
        assert_eq!(keys.take_requests().len(), 1);
    }

    #[test]
    fn only_mods_with_hotkeys_turn_over() {
        let mut keys = CardKeys::default();
        keys.receive(answer("none", 0));
        keys.receive(answer("some", 2));
        assert!(!keys.has("none"));
        assert!(!keys.has("unknown"));
        assert!(keys.has("some"));
        keys.toggle("none", 0.0);
        assert_eq!(keys.face("none", 1.0), Face::FRONT);
        keys.toggle("some", 0.0);
        assert!(!keys.face("some", 0.0).back);
        let turned = keys.face("some", FLIP_SECS);
        assert!(turned.back && turned.settled());
    }

    #[test]
    fn turning_back_midway_starts_from_where_it_is() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 1));
        keys.toggle("some", 0.0);
        let quarter = FLIP_SECS * 0.25;
        let before = keys.face("some", quarter);
        keys.toggle("some", quarter);
        let after = keys.face("some", quarter);
        assert!(!before.back && !after.back);
        assert!((before.width - after.width).abs() < 1e-4);
        // It's back on its front a quarter of the time later.
        assert_eq!(keys.face("some", quarter * 2.0 + 1e-6), Face::FRONT);
        assert!(keys.flip.is_none());
    }

    #[test]
    fn losing_the_focus_shows_the_front() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 1));
        keys.toggle("some", 0.0);
        keys.keep_focused(Some("some"));
        assert!(keys.face("some", FLIP_SECS).back);
        keys.keep_focused(Some("other"));
        assert_eq!(keys.face("some", FLIP_SECS), Face::FRONT);
    }
}
